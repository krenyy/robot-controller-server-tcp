mod logging;
mod robot;
mod util;

use crate::robot::RobotController;
use std::io;
use tokio::net::{TcpListener, TcpStream};
use tracing::Instrument;

async fn handle_client(stream: TcpStream) {
    let addr = stream.peer_addr().unwrap();
    RobotController::start(stream)
        .instrument(tracing::trace_span!("robot", addr = addr.to_string()))
        .await;
}

#[tokio::main]
async fn main() -> io::Result<()> {
    logging::set_up();

    let addr = "0.0.0.0:3000";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(handle_client(socket));
    }
}
