use std::io::Write;
use libtailscale::Tailscale;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ts = Tailscale::new();
    //ts.set_ephemeral(true)?;
    ts.set_control_url("https://michiscale.yellows.ink")?;
    ts.set_hostname("milkzel-wakerscale")?;
    ts.up()?;

    let listener = ts.listen("tcp", "[::]:8000")?;

    loop {
        let mut conn = listener.accept()?;
        conn.write_all("meow ^w^".as_bytes())?;
    }
}
