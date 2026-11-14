//! Stand-alone THipPro simulator for development and testing.
//!
//!     cargo run -p linecaller-core --bin rlp-sim -- [--bind 127.0.0.1:10100] [--location 1]
//!
//! Point LineCaller (simulator mode off) at the printed address. It speaks the
//! Remote Light Protocol and places no real calls. Type a line number (1-16)
//! and press Enter to make that line ring.

use linecaller_core::Simulator;
use std::io::BufRead;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut bind = "127.0.0.1:10100".to_string();
    let mut location = 1u8;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--bind" => bind = args.next().expect("--bind needs an address"),
            "--location" => {
                location = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--location needs 1-6")
            }
            "-h" | "--help" => {
                println!("rlp-sim [--bind ADDR:PORT] [--location N]");
                return Ok(());
            }
            other => panic!("unknown argument {other}"),
        }
    }
    let sim = Simulator::start(bind.parse().expect("invalid --bind address"), location).await?;
    println!("Remote Light simulator listening on {}", sim.addr());
    println!("Type a line number (1-16) + Enter to ring that line. Ctrl+C to quit.");
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(8);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if let Ok(n) = line.trim().parse::<u8>() {
                let _ = tx.blocking_send(n);
            }
        }
    });
    loop {
        tokio::select! {
            Some(n) = rx.recv() => {
                match n.checked_sub(1).and_then(|i| rlp::LineIndex::new(i).ok()) {
                    Some(li) if sim.incoming_call(li) => println!("Line {n} is ringing"),
                    Some(_) => println!("Line {n} is busy or locked"),
                    None => println!("Line must be 1-16"),
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}
