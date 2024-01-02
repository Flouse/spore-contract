use alloc::{format, vec, vec::Vec};
use ckb_std::ckb_types::util::hash::blake2b_256;
use core::ffi::CStr;
use core::result::Result;

use ckb_std::ckb_constants::Source::{CellDep, GroupInput, GroupOutput, Input, Output};
use ckb_std::ckb_types::core::ScriptHashType;
use ckb_std::ckb_types::packed::Script;
use ckb_std::high_level::{load_cell_data_hash, load_cell_lock_hash, load_script_hash};
use ckb_std::{
    ckb_constants::Source,
    ckb_types::prelude::*,
    high_level::{load_cell_data, load_cell_type, QueryIter},
};

use spore_errors::error::Error;
use spore_types::generated::spore_types::{ClusterData, SporeData};
use spore_utils::{
    calc_capacity_sum, find_position_by_lock_hash, find_position_by_type,
    find_position_by_type_args, load_type_args, verify_type_id, MIME,
};

use crate::hash::{CLUSTER_AGENT_CODE_HASHES, CLUSTER_CODE_HASHES};
use crate::schema;

fn load_spore_data(index: usize, source: Source) -> Result<SporeData, Error> {
    let raw_data = load_cell_data(index, source)?;
    let spore_data =
        SporeData::from_compatible_slice(raw_data.as_slice()).map_err(|_| Error::InvalidNFTData)?;
    Ok(spore_data)
}

fn check_spore_address(
    gourp_source: Source,
    spore_address: Option<schema::Address>,
) -> Result<(), Error> {
    if let Some(expected_address) = spore_address {
        let address = load_cell_lock_hash(0, gourp_source)?;
        if address.as_slice() != expected_address.as_slice() {
            return Err(Error::SporeActionFieldMismatch);
        }
    }
    Ok(())
}

fn process_creation(index: usize, action: schema::SporeAction) -> Result<(), Error> {
    let spore_data = load_spore_data(index, Output)?;

    if spore_data.content().is_empty() {
        return Err(Error::EmptyContent);
    }

    let content = spore_data.content();
    let content_arr = content.as_slice();

    if spore_data.content_type().is_empty() {
        return Err(Error::InvalidContentType);
    }

    // verify NFT ID
    let Some(spore_id) = verify_type_id(index, Output) else {
        return Err(Error::InvalidNFTID);
    };

    let raw_content_type = spore_data.content_type();
    let content_type = raw_content_type.unpack();

    let mime = MIME::parse(content_type)?; // content_type validation
    if content_type[mime.main_type.clone()] == "multipart".as_bytes()[..] {
        // Check if boundary param exists
        let boundary_range = mime
            .get_param(content_type, "boundary")
            .ok_or(Error::InvalidContentType)?;
        kmp::kmp_find(
            format!(
                "--{}",
                alloc::str::from_utf8(&content_type[boundary_range]).or(Err(Error::Encoding))?
            )
            .as_bytes(),
            content_arr,
        )
        .ok_or(Error::InvalidMultipartContent)?;
    }

    if spore_data.cluster_id().to_opt().is_some() {
        // check if cluster cell in deps
        let cluster_id = spore_data
            .cluster_id()
            .to_opt()
            .unwrap_or_default()
            .raw_data();
        let cluster_fn: fn(&[u8; 32]) -> bool = |x| -> bool { CLUSTER_CODE_HASHES.contains(x) };
        let agent_fn: fn(&[u8; 32]) -> bool = |x| -> bool { CLUSTER_AGENT_CODE_HASHES.contains(x) };
        let cell_dep_index = find_position_by_type_args(&cluster_id, CellDep, Some(cluster_fn))
            .ok_or(Error::ClusterCellNotInDep)?;

        let raw_cluster_data = load_cell_data(cell_dep_index, CellDep)?;
        let cluster_data =
            ClusterData::from_compatible_slice(&raw_cluster_data).unwrap_or_default(); // the cluster contract guarantees the cluster data will always be correct once created
        if cluster_data.mutant_id().is_some() {
            let mutant_id = cluster_data.mutant_id().to_opt().unwrap_or_default();
            let mutant_verify_passed = mime
                .mutants
                .iter()
                .any(|mutant| mutant == mutant_id.raw_data().as_ref());
            if !mutant_verify_passed {
                // required mutant does not applied
                return Err(Error::ClusterRequiresMutantApplied);
            }
        }

        // Condition 1: Check if cluster exist in Inputs & Outputs
        return if find_position_by_type_args(&cluster_id, Input, Some(cluster_fn)).is_some()
            && find_position_by_type_args(&cluster_id, Output, Some(cluster_fn)).is_some()
        {
            Ok(())
        }
        // Condition 2: Check if cluster agent in Inputs & Outputs
        else if find_position_by_type_args(&cluster_id, Input, Some(agent_fn)).is_some()
            && find_position_by_type_args(&cluster_id, Output, Some(agent_fn)).is_some()
        {
            Ok(())
        }
        // Condition 3: Use cluster agent by lock proxy
        else if let Some(agent_index) =
            find_position_by_type_args(&cluster_id, CellDep, Some(agent_fn))
        {
            let agent_lock_hash = load_cell_lock_hash(agent_index, CellDep)?;
            find_position_by_lock_hash(&agent_lock_hash, Output)
                .ok_or(Error::ClusterOwnershipVerifyFailed)?;
            find_position_by_lock_hash(&agent_lock_hash, Input)
                .ok_or(Error::ClusterOwnershipVerifyFailed)?;
            Ok(())
        } else {
            // Condition 4: Check if Lock Proxy exist in Inputs & Outputs
            let cluster_lock_hash = load_cell_lock_hash(cell_dep_index, CellDep)?;
            find_position_by_lock_hash(&cluster_lock_hash, Output)
                .ok_or(Error::ClusterOwnershipVerifyFailed)?;
            find_position_by_lock_hash(&cluster_lock_hash, Input)
                .ok_or(Error::ClusterOwnershipVerifyFailed)?;
            Ok(())
        };
    }

    if !mime.mutants.is_empty() {
        verify_extension(&mime, 0, vec![index as u8])?;
    }

    // check co-build action @lyk
    let schema::SporeActionUnion::Mint(mint) = action.to_enum() else {
        return Err(Error::SporeActionMismatch);
    };
    if mint.nft_id().as_slice() != spore_id
        || mint.data_hash().as_slice() != blake2b_256(spore_data.as_slice())
    {
        return Err(Error::SporeActionFieldMismatch);
    }
    check_spore_address(GroupOutput, mint.to().to_opt())?;

    Ok(())
}

fn process_destruction(action: schema::SporeAction) -> Result<(), Error> {
    //destruction
    let spore_data = load_spore_data(0, GroupInput)?;

    let content_type_bytes = spore_data.content_type();
    let content_type = content_type_bytes.unpack();
    let mime = MIME::parse(content_type)?;
    if mime.immortal {
        // true destroy a immortal nft
        return Err(Error::DestroyImmortalNFT);
    }

    if !mime.mutants.is_empty() {
        let type_script = load_cell_type(0, GroupInput)?.unwrap_or_default();
        let index = find_position_by_type(&type_script, Input).ok_or(Error::IndexOutOfBound)?;
        verify_extension(&mime, 2, vec![index as u8])?;
    }

    // check co-build action @lyk
    let schema::SporeActionUnion::Burn(burn) = action.to_enum() else {
        return Err(Error::SporeActionMismatch);
    };
    if burn.nft_id().as_slice() != load_type_args(0, GroupInput).as_ref() {
        return Err(Error::SporeActionFieldMismatch);
    }
    check_spore_address(GroupInput, burn.from().to_opt())?;

    Ok(())
}

fn process_transfer(action: schema::SporeAction) -> Result<(), Error> {
    // found same NFT in output, this is a transfer
    // check no field was modified
    let input_data = load_spore_data(0, GroupInput)?;
    let output_data = load_spore_data(0, GroupOutput)?;

    if input_data.as_slice()[..] != output_data.as_slice()[..] {
        return Err(Error::ModifySporePermanentField);
    }

    let content_type_bytes = input_data.content_type();
    let content_type = content_type_bytes.unpack();
    let mime = MIME::parse(content_type)?;

    if !mime.mutants.is_empty() {
        let type_script = load_cell_type(0, GroupInput)?.unwrap_or_default();
        let input_index =
            find_position_by_type(&type_script, Input).ok_or(Error::IndexOutOfBound)?;
        let output_index =
            find_position_by_type(&type_script, Output).ok_or(Error::IndexOutOfBound)?;
        verify_extension(&mime, 1, vec![input_index as u8, output_index as u8])?;
    }

    // check co-build action @lyk
    let schema::SporeActionUnion::Transfer(transfer) = action.to_enum() else {
        return Err(Error::SporeActionMismatch);
    };
    if transfer.nft_id().as_slice() != load_type_args(0, GroupInput).as_ref() {
        return Err(Error::SporeActionFieldMismatch);
    }
    check_spore_address(GroupInput, transfer.to().to_opt())?;
    check_spore_address(GroupOutput, transfer.from().to_opt())?;

    Ok(())
}

fn verify_extension(mime: &MIME, op: usize, argv: Vec<u8>) -> Result<(), Error> {
    for mutant in mime.mutants.iter() {
        let ext_pos = QueryIter::new(load_cell_type, CellDep).position(|script| match script {
            Some(script) => mutant[..] == script.args().raw_data()[..32],
            None => false,
        });
        match ext_pos {
            None => return Err(Error::ExtensionCellNotInDep),
            Some(ext_pos) => {
                if op == 0 {
                    check_payment(ext_pos)?;
                }

                let ext_pos = ext_pos as u8;
                let code_hash = load_cell_data_hash(ext_pos.into(), CellDep)?;
                match op {
                    0 | 2 => {
                        ckb_std::high_level::exec_cell(
                            &code_hash,
                            ScriptHashType::Data1,
                            &[
                                CStr::from_bytes_with_nul([b'0', 0].as_slice()).unwrap_or_default(),
                                CStr::from_bytes_with_nul([b'0' + ext_pos, 0].as_slice())
                                    .unwrap_or_default(),
                                CStr::from_bytes_with_nul([b'0' + argv[0], 0].as_slice())
                                    .unwrap_or_default(),
                            ],
                        )?;
                    }
                    1 => {
                        ckb_std::high_level::exec_cell(
                            &code_hash,
                            ScriptHashType::Data1,
                            &[
                                CStr::from_bytes_with_nul([b'0', 0].as_slice()).unwrap_or_default(),
                                CStr::from_bytes_with_nul([b'0' + ext_pos, 0].as_slice())
                                    .unwrap_or_default(),
                                CStr::from_bytes_with_nul([b'0' + argv[0], 0].as_slice())
                                    .unwrap_or_default(),
                                CStr::from_bytes_with_nul([b'0' + argv[1], 0].as_slice())
                                    .unwrap_or_default(),
                            ],
                        )?;
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(())
}

fn check_payment(ext_pos: usize) -> Result<(), Error> {
    let ext_script = load_cell_type(ext_pos, CellDep)?.unwrap_or_default();
    let ext_args = ext_script.args().raw_data();
    // CAUTION: only check 33 size pattern, leave room for user customizing
    if ext_args.len() > 32 {
        // we need a payment
        let lock = load_cell_lock_hash(ext_pos, CellDep)?;

        let input_capacity = calc_capacity_sum(&lock, Input);
        let output_capacity = calc_capacity_sum(&lock, Output);
        let minimal_payment = 10u128.pow(ext_args.get(32).cloned().unwrap_or(0) as u32);
        if input_capacity + minimal_payment < output_capacity {
            return Err(Error::ExtensionPaymentNotEnough);
        }
    }
    Ok(())
}

pub fn main() -> Result<(), Error> {
    let spore_in_output: Vec<Script> = QueryIter::new(load_cell_type, GroupOutput)
        .map(|script| script.unwrap_or_default())
        .collect();

    if spore_in_output.len() > 1 {
        return Err(Error::ConflictCreation);
    }

    let spore_in_input: Vec<Script> = QueryIter::new(load_cell_type, GroupInput)
        .map(|script| script.unwrap_or_default())
        .collect();

    if spore_in_input.len() > 1 {
        return Err(Error::MultipleSpend);
    }

    let message = ckb_transaction_cobuild::fetch_message()
        .map_err(|_| Error::InvliadCoBuildWitnessLayout)?
        .ok_or(Error::InvliadCoBuildWitnessLayout)?;

    let script_hash = load_script_hash()?;
    let action = message
        .actions()
        .into_iter()
        .find(|v| v.script_hash().as_slice() == script_hash.as_slice())
        .ok_or(Error::InvliadCoBuildMessage)?;
    let spore_action = schema::SporeAction::from_slice(&action.data().raw_data())
        .map_err(|_| Error::InvliadCoBuildMessage)?;

    match (spore_in_input.len(), spore_in_output.len()) {
        (0, 1) => {
            // find it's index in Output
            let output_index =
                find_position_by_type(&spore_in_output[0], Output).unwrap_or_default(); // Once we entered here, it can't be empty, and use 0 as a fallback position
            return process_creation(output_index, spore_action);
        }
        (1, 0) => {
            return process_destruction(spore_action);
        }
        (1, 1) => {
            return process_transfer(spore_action);
        }
        _ => unreachable!(),
    }
}
