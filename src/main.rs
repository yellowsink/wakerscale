use bytes::Bytes;
use facet::Facet;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use libtailscale::Tailscale;
use regex::Regex;
use std::fs;
use std::sync::LazyLock;

// get allowed passwords, port number, etc
#[derive(Facet)]
struct Config {
	hostname: String,
	//iface: String,
	controlserver: Option<String>,
	port: usize,
	passwords: Vec<String>,
}

static CFG: LazyLock<Config> = LazyLock::new(|| {
	facet_json::from_str(
		fs::read_to_string("./wakerscale.json")
			.expect("failed to read wakerscale.json")
			.as_str(),
	)
	.expect("failed to deserialize wakerscale.json")
});

// write tokio::main ourselves to be free of syn ✨
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let rt = tokio::runtime::Builder::new_current_thread().enable_io().build()?;
	rt.block_on(async { main_async().await })
}

async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
	let mut ts = Tailscale::new();
	ts.set_ephemeral(true)?;
	ts.set_hostname(&CFG.hostname)?;

	if let Some(cs) = &CFG.controlserver {
		ts.set_control_url(cs)?;
	}

	ts.up()?;
	let ts = ts;

	// ts listener is blocking so we need a thread :(
	let (tx, mut rx) = tokio::sync::mpsc::channel(3);

	let thr = std::thread::spawn(move || {
		let listener = ts.listen("tcp", &format!(":{}", CFG.port)).unwrap();

		for stream in listener.incoming() {
			match stream {
				Ok(stream) => {
					tx.blocking_send(stream).unwrap();
				}
				Err(e) => {
					eprintln!("accept error: {e}");
				}
			}
		}
	});

	println!("listening on {}", CFG.port);

	while let Some(stream) = rx.recv().await {
		// convert to tokio stream
		stream.set_nonblocking(true)?;
		let stream = tokio::net::TcpStream::from_std(stream)?;

		// hand to hyper
		tokio::spawn(async move {
			if let Err(e) = http1::Builder::new()
				.serve_connection(TokioIo::new(stream), service_fn(handle))
				.await
			{
				eprintln!("error serving connection: {e}");
			}
		});
	}

	// explicitly keep the worker thread in scope to stop us from join()ing it too early via drop
	drop(thr);
	Ok(())
}

// handle request
async fn handle(
	req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
	match (req.method(), req.uri().path()) {
		(&Method::POST, "/wake") => {
			// are you authed?
			if let Some(password) = req.headers().get("Authorization") {
				if CFG.passwords.iter().all(|p| p != password) {
					let mut r = bullshit_to_200_ok("Password is not correct");
					*r.status_mut() = StatusCode::UNAUTHORIZED;
					Ok(r)
				} else {
					// get mac address
					let mac = req.uri().query();
					if let Some(mac) = mac {
						match parse_mac_address_from_query(mac) {
							Ok(mac) => {
								// send WOL packet
								let packet = wake_on_lan::MagicPacket::new(&mac);
								let res = packet.send();

								if let Err(e) = res {
									let mut r = bullshit_to_200_ok(format!("Error sending WOL packet: {e}"));
									*r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
									Ok(r)
								} else {
									// 200 OK
									Ok(bullshit_to_200_ok(""))
								}
							}
							Err(err) => {
								let mut r = bullshit_to_200_ok(err);
								*r.status_mut() = StatusCode::BAD_REQUEST;
								Ok(r)
							}
						}
					}
					else {
						let mut r = bullshit_to_200_ok("Missing mac address, /wake?mac=XX:XX:XX:XX:XX:XX");
						*r.status_mut() = StatusCode::BAD_REQUEST;
						Ok(r)
					}
				}
			} else {
				let mut r = bullshit_to_200_ok("Missing password");
				*r.status_mut() = StatusCode::UNAUTHORIZED;
				Ok(r)
			}
		}

		_ => {
			let mut resp = bullshit_to_200_ok("404 lol bozo");
			*resp.status_mut() = StatusCode::NOT_FOUND;

			Ok(resp)
		}
	}
}

fn bullshit_to_200_ok(shit: impl Into<Bytes>) -> Response<BoxBody<Bytes, hyper::Error>> {
	Response::new(
		Full::new(shit.into())
			// convert Infallible to hyper::Error
			.map_err(|n| match n {})
			.boxed(),
	)
}

// this may be the worst rust ive ever written
fn parse_mac_address_from_query(mac: &str) -> Result<[u8; 6], &'static str> {



	let re = Regex::new("mac=((?:[0-9a-f]{2}:){5}[0-9a-f]{2})").unwrap();

	let caps = re.captures(mac);
	if caps.is_some() && caps.as_ref().unwrap().get(1).is_some() {
		let cap = caps.unwrap().get(1).unwrap();

		let parts = mac[cap.range()].split(":")
			.map(|part| {
				let nested_result = hex::decode(part)
					.map(|part| { part.get(0).cloned().ok_or("Invalid MAC address") });

				nested_result.map_err(|_| "Invalid hex").and_then(|v| v)
			});

		// unwrap errors
		let mut unwrapped = Vec::with_capacity(6);
		let mut error = None;
		for res in parts {
			if let Ok(chunk) = res {
				unwrapped.push(chunk);
			}
			else {
				error = Some(res.unwrap_err());
			}
		}

		if let Some(err) = error {
			Err(err)
		}
		else if unwrapped.len() != 6 {
			Err("wrong number of chunks in MAC")
		} else {
			Ok(unwrapped.first_chunk::<6>().unwrap().clone())
		}
	} else {
		Err("Invalid MAC address format, /wake?mac=XX:XX:XX:XX:XX:XX")
	}
}