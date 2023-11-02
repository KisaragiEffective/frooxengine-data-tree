use std::process::ExitCode;
use frooxengine_data_tree::DeserializeError;

fn main() -> ExitCode {
    match main_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn main_inner() -> Result<(), DeserializeError> {
    let file = std::env::args().nth(1).expect("Usage: [path-to-file]");
    let file = std::fs::read(file)?;
    let m = frooxengine_data_tree::split_froox_container_header(&file).unwrap_or_else(|_| frooxengine_data_tree::legacy(&file));
    let b = m.deserialize::<bson::Bson>()?;
    println!("{b:?}");

    Ok(())
}
