use std::fs;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use ckb_testtool::ckb_types::bytes::Bytes;

enum Target {
    Debug,
    Release,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Debug => write!(f, "debug"),
            Target::Release => write!(f, "release"),
        }
    }
}

fn main() {
    // Please capsule build using ckb toolchain image: nervos/ckb-riscv-gnu-toolchain:focal-20230214
    // make build

    // record_code_hash in ./debug and ./release
    record_code_hash(Target::Debug);
    record_code_hash(Target::Release);
}

/// record the contract binaries' code_hashes
fn record_code_hash(target: Target) {
    const CONTRACT_NAMES: [&str; 5] = [
        "spore",
        "cluster",
        "cluster_agent",
        "cluster_proxy",
        "spore_extension_lua",
    ];

    let cur_dir = env::current_dir().unwrap();
    let mut file_path = PathBuf::new();
    file_path.push(cur_dir);
    file_path.push("code_hash.md");

    let mut file = match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening the file: {}", e);
            return;
        }
    };

    // get git commit of the contracts
    if let Some(commit_id) = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--exclude", "*"])
        .output()
        .ok()
        .and_then(|r| String::from_utf8(r.stdout).ok())
    {
        let commit_id = commit_id.trim();
        let output = format!("\n## commit: {commit_id} ({target})\n", );
        print!("{output}");
        file.write(output.as_bytes()).unwrap();
    }

    for contract_name in &CONTRACT_NAMES {
        let bin = load_binary(contract_name, &target);
        let code_hash = ckb_hash::blake2b_256(&bin);
        let output = format!(
            "- code_hash of contact {:<19}: 0x{}\n",
            contract_name,
            hex::encode(&code_hash)
        );
        print!("{output}");
        file.write(output.as_bytes()).unwrap();
    }
}

fn load_binary(name: &str, target: &Target) -> Bytes {
    let cur_dir = env::current_dir().unwrap();
    let mut file_path = PathBuf::new();
    file_path.push(cur_dir);
    file_path.push(target.to_string());
    file_path.push(name);
    fs::read(file_path).expect("load contract binary").into()
}
