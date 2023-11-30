use autometrics::prometheus_exporter;
use axum::{routing::get, Router};
use server::{MyJobRunner};
use std::net::SocketAddr;
use tonic::transport::Server;
use tokio::{
    signal::unix::{signal, SignalKind}, spawn,
    sync::oneshot::{self, Receiver, Sender},
};
use crate::server::job::job_runner_server::JobRunnerServer;

mod server;

#[tokio::main]
async fn main() {
    // Set up the exporter to collect metrics
    prometheus_exporter::init();

    let grpc_addr = "127.0.0.1:50051".parse().unwrap();
    let web_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    // gRPC server
    let grpc_service = JobRunnerServer::new(MyJobRunner::default());

    // Construct health service for gRPC server
    let (mut health_reporter, health_svc) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<JobRunnerServer<MyJobRunner>>().await;


    spawn(async move {
        Server::builder()
            .add_service(grpc_service)
            .add_service(health_svc)
            .serve(grpc_addr)
            .await
            .expect("gRPC server failed");
    });

    // Web server with Axum
    let app = Router::new().route("/", get(handler)).route(
        "/metrics",
        get(|| async { prometheus_exporter::encode_http_response() }),
    );

    axum::Server::bind(&web_addr)
        .serve(app.into_make_service())
        .await
        .expect("Web server failed");
}

async fn handler() -> &'static str {
    "Hello, World!"
}
