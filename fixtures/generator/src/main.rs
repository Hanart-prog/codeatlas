//! Writes a fixture repository to the directory given as an argument.

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str =
    "usage: generate-fixture <empty-directory> [--demo | --scale <files> <commits>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args[..] {
        [directory] => atlas_fixtures::generate_main_fixture(Path::new(directory)),
        [directory, "--demo"] => atlas_fixtures::generate_demo_fixture(Path::new(directory)),
        [directory, "--scale", files, commits] => {
            let (Ok(files), Ok(commits)) = (files.parse(), commits.parse()) else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            atlas_fixtures::generate_scale_fixture(Path::new(directory), files, commits)
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
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
