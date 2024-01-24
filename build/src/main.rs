use ckb_testtool::ckb_types::bytes::Bytes;
use std::path::PathBuf;
use std::{env, fs};

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

const CONTRACT_NAMES: [&str; 5] = [
    "spore",
    "cluster",
    "cluster_proxy",
    "cluster_agent",
    "spore_extension_lua",
];

/// Please run `make build` first
/// which is using ckb toolchain image: nervos/ckb-riscv-gnu-toolchain:focal-20230214
fn main() {
    println!("### Record code_hash of contract binaries in build/debug:");
    record_code_hash(Target::Debug);

    print!("\n---\n");

    println!("### Record code_hash of contract binaries in build/release:");
    record_code_hash(Target::Release);
}

/// record the contract binaries' code_hashes
fn record_code_hash(target: Target) {
    for contract_name in &CONTRACT_NAMES {
        let bin = match load_binary(contract_name, &target) {
            Ok(bin) => bin,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!(
                    "Contract {}'s binary not found in build/{} dir.",
                    contract_name, target,
                );
                break;
            }
            Err(e) => {
                eprintln!("Error when loading the binary: {}", e);
                break;
            }
        };
        let code_hash = ckb_hash::blake2b_256(&bin);
        let output = format!(
            "- code_hash of contact {:<19}: 0x{}\n",
            contract_name,
            hex::encode(code_hash)
        );
        print!("{output}");
    }
}

fn load_binary(name: &str, target: &Target) -> Result<Bytes, std::io::Error> {
    let cur_dir = env::current_dir().unwrap();
    let mut file_path = PathBuf::new();
    file_path.push(cur_dir);
    file_path.push(target.to_string());
    file_path.push(name);
    fs::read(file_path).map(Bytes::from)
}
