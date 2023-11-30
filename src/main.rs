use std::net::SocketAddr;
use autometrics::prometheus_exporter;
use axum::{Router, routing::get};
use tokio::{
    signal::unix::{signal, SignalKind}, spawn,
    sync::oneshot::{self, Receiver, Sender},
};
use tonic::transport::Server as TonicServer;
use server::MyJobRunner;
use crate::server::job::job_runner_server::JobRunnerServer;

mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up the exporter to collect metrics
    prometheus_exporter::init();

    let grpc_addr = "127.0.0.1:50051".parse().expect("Failed to parse gRPC address");
    let web_addr: SocketAddr = "127.0.0.1:8080".parse().expect("Failed to parse web address");

    // gRPC server
    let svc = JobRunnerServer::new(MyJobRunner::default());

    // Construct health service for gRPC server
    let (mut health_reporter, health_svc) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<JobRunnerServer<MyJobRunner>>().await;

    // Construct sigint signal handler for graceful shutdown
    let (signal_tx, signal_rx) = signal_channel();
    spawn(handle_sigterm(signal_tx));

    // Build gRPC server with health service and signal sigint handler
    let server = TonicServer::builder()
        .add_service(svc)
        .add_service(health_svc)
        .serve_with_shutdown(grpc_addr, async {
            signal_rx.await.ok();
        });

    // Start gRPC servedr
    // This one probably blocks the subsequent start of the web server. How do I start them either in Tandem?
    println!("Server listening on {}", grpc_addr);
    server
        .await
        .expect("Failed to start server");

    // Http handler that exposes metrics to Prometheus
    let app = Router::new().route("/", get(handler)).route(
        "/metrics",
        get(|| async { prometheus_exporter::encode_http_response() }),
    );

    // Web server with Axum
    // How do I add a graceful shutdown signal handler
    // that triggers a proper shutdown together with the gRPC server?
    axum::Server::bind(&web_addr)
        .serve(app.into_make_service())
        .await
        .expect("Web server failed");

    Ok(())
}

async fn handler() -> &'static str {
    "Hello, World!"
}


fn signal_channel() -> (Sender<()>, Receiver<()>) {
    oneshot::channel()
}

async fn handle_sigterm(tx: Sender<()>) {
    let _ = signal(SignalKind::terminate())
        .expect("failed to install signal handler")
        .recv()
        .await;

    println!("SIGTERM received: shutting down");
    let _ = tx.send(());
}
