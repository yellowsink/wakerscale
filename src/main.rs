use bytes::Bytes;
use facet::Facet;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use libtailscale::Tailscale;
use std::fs;
use std::sync::LazyLock;

// get allowed passwords, port number, etc
#[derive(Facet)]
struct Config {
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
	ts.set_control_url("https://michiscale.yellows.ink")?;
	ts.set_hostname("milkzel-wakerscale")?;
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
					// WE'RE GOOD
					Ok(bullshit_to_200_ok("TODO: implement functionality, but you're in."))
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
