use std::net::SocketAddr;

use autometrics::prometheus_exporter;
use tokio::signal::unix::{signal, SignalKind};
use tonic::transport::Server as TonicServer;
use warp::Filter;

use server::MyJobRunner;
use crate::db_manager::DBManager;

use crate::server::job::job_runner_server::JobRunnerServer;

mod server;
mod db_manager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up prometheus metrics exporter
    prometheus_exporter::init();

    // Set up two different ports for gRPC and HTTP
    let grpc_addr = "127.0.0.1:50051".parse().expect("Failed to parse gRPC address");
    let web_addr: SocketAddr = "127.0.0.1:8080".parse().expect("Failed to parse web address");

    // Build new DBManager that connects to the database
    let dbm = db_manager::DBManager::new();
    // Connect to the database
    dbm.connect_to_db().await.expect("Failed to connect to database");

    // gRPC server with DBManager
    let grpc_svc = JobRunnerServer::new(MyJobRunner::new(dbm));

    // Sigint signal handler that closes the DB connection upon shutdown
    let signal = grpc_sigint(dbm.clone());

    // Construct health service for gRPC server
    let (mut health_reporter, health_svc) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<JobRunnerServer<MyJobRunner>>().await;

    // Build gRPC server with health service and signal sigint handler
    let grpc_server = TonicServer::builder()
        .add_service(grpc_svc)
        .add_service(health_svc)
        .serve_with_shutdown(grpc_addr, signal);

    // Build http /metrics endpoint
    let routes = warp::get()
        .and(warp::path("metrics"))
        .map(|| prometheus_exporter::encode_http_response());

    // Build http web server
    let (_, web_server) = warp::serve(routes)
        .bind_with_graceful_shutdown(web_addr, http_sigint());

    // Create handler for each server
    //  https://github.com/hyperium/tonic/discussions/740
    let grpc_handle = tokio::spawn(grpc_server);
    let grpc_web_handle = tokio::spawn(web_server);

    println!("Started gRPC server on port {:?} and metrics on port {:?}", grpc_addr.port(), web_addr.port());
    // Join all servers together and start the the main loop
    let _ = tokio::try_join!(grpc_handle, grpc_web_handle)
        .expect("Failed to start gRPC and http server");

    Ok(())
}

// Signal sender is non-clonable therefore we need to create a new one for each server.
// https://github.com/rust-lang/futures-rs/issues/1971
async fn http_sigint() {
    let _ = signal(SignalKind::terminate())
        .expect("failed to create a new SIGINT signal handler for htttp")
        .recv()
        .await;

    println!("http shutdown complete");
}

async fn grpc_sigint(dbm: DBManager) {
    let _ = signal(SignalKind::terminate())
        .expect("failed to create a new SIGINT signal handler for gRPC")
        .recv()
        .await;

    // Shutdown the DB connection.
    dbm.close_db().await.expect("Failed to close database connection");

    println!("gRPC shutdown complete");
}
