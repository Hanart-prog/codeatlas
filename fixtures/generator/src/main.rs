//! Writes the main fixture repository to the directory given as an argument.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(directory) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: generate-fixture <empty-directory>");
        return ExitCode::from(2);
    };
    match atlas_fixtures::generate_main_fixture(&directory) {
        Ok(tip) => {
            println!("{tip}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
