use std::net::SocketAddr;

use autometrics::prometheus_exporter;
use tokio::signal::unix::{signal, SignalKind};
use tonic::transport::Server as TonicServer;
use warp::Filter;

use server::MyJobRunner;

use crate::server::job::job_runner_server::JobRunnerServer;

mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up prometheus metrics exporter
    prometheus_exporter::init();

    // Set up two different ports for gRPC and HTTP
    let grpc_addr = "127.0.0.1:50051".parse().expect("Failed to parse gRPC address");
    let web_addr: SocketAddr = "127.0.0.1:8080".parse().expect("Failed to parse web address");

    // gRPC server
    let svc = JobRunnerServer::new(MyJobRunner::default());

    // Construct health service for gRPC server
    let (mut health_reporter, health_svc) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<JobRunnerServer<MyJobRunner>>().await;

    // Build gRPC server with health service and signal sigint handler
    let grpc_server = TonicServer::builder()
        .add_service(svc)
        .add_service(health_svc)
        .serve_with_shutdown(grpc_addr, grpc_sigint());

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

async fn grpc_sigint() {
    let _ = signal(SignalKind::terminate())
        .expect("failed to create a new SIGINT signal handler for gRPC")
        .recv()
        .await;

    println!("gRPC shutdown complete");
}
