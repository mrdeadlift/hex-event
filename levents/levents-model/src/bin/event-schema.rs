use levents_model::schema;
use std::error::Error;
use std::io::{self, Write};

fn main() {
    if let Err(error) = run() {
        let _ = writeln!(io::stderr(), "error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let schema = schema::event_batch_schema();

    if let Some(path) = std::env::args().nth(1) {
        schema::write_event_batch_schema(path)?;
    } else {
        serde_json::to_writer_pretty(std::io::stdout(), &schema)?;
    }

    Ok(())
}
