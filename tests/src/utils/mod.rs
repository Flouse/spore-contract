#![allow(dead_code)]

use ckb_testtool::builtin::ALWAYS_SUCCESS;
use ckb_testtool::ckb_hash::{new_blake2b, Blake2bBuilder};
use ckb_testtool::ckb_types::core::ScriptHashType;
use ckb_testtool::ckb_types::{
    bytes::Bytes, core::TransactionBuilder, core::TransactionView, packed, packed::*, prelude::*,
    H256,
};
use ckb_testtool::context::Context;
use std::num::ParseIntError;

use spore_types::generated::spore_types::{ClusterData, SporeData};
use spore_types::NativeNFTData;

use crate::Loader;

pub mod co_build;
mod internal;

pub fn build_serialized_spore(nft_content: &str, nft_type: &str) -> SporeData {
    build_serialized_spore_internal(nft_content.as_bytes().to_vec(), nft_type, None)
}

pub fn build_serialized_spore_internal(
    nft_content: Vec<u8>,
    nft_type: &str,
    cluster_id: Option<Vec<u8>>,
) -> SporeData {
    let nft = NativeNFTData {
        content: nft_content,
        content_type: nft_type.to_owned(),
        cluster_id,
    };
    SporeData::from(nft)
}

pub fn build_type_id(first_input: &CellInput, out_index: usize) -> [u8; 32] {
    let mut blake2b = Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();
    blake2b.update(first_input.as_slice());
    blake2b.update(&(out_index).to_le_bytes());
    let mut verify_id = [0; 32];
    blake2b.finalize(&mut verify_id);
    verify_id
}

pub fn build_spore_type_script(
    context: &mut Context,
    out_point: &OutPoint,
    args: Bytes,
) -> Option<Script> {
    context.build_script_with_hash_type(out_point, ScriptHashType::Data1, args)
}

pub fn build_spore_input(
    context: &mut Context,
    spore_type: Option<Script>,
    spore_data: SporeData,
) -> CellInput {
    let input_ckb = spore_data.total_size() as u64;
    internal::build_input(
        context,
        input_ckb,
        spore_type,
        Bytes::copy_from_slice(spore_data.as_slice()),
    )
}

pub fn build_cluster_input(
    context: &mut Context,
    cluster_data: ClusterData,
    type_: Option<Script>,
) -> CellInput {
    let input_ckb = cluster_data.total_size() as u64;
    internal::build_input(
        context,
        input_ckb,
        type_,
        Bytes::copy_from_slice(cluster_data.as_slice()),
    )
}

pub fn build_normal_input(context: &mut Context, capacity: u64) -> CellInput {
    internal::build_input(context, capacity, None, Bytes::new())
}

pub fn build_output_cell_with_type_id(
    context: &mut Context,
    capacity: u64,
    type_: Option<Script>,
) -> CellOutput {
    internal::build_output(context, capacity, type_)
}

pub fn build_normal_output(context: &mut Context, capasity: u64) -> CellOutput {
    internal::build_output(context, capasity, None)
}

pub fn build_normal_cell_dep(context: &mut Context, data: &[u8], type_: Option<Script>) -> CellDep {
    let outpoint = internal::build_outpoint(
        context,
        data.len() as u64,
        type_,
        Bytes::copy_from_slice(data),
    );
    CellDep::new_builder().out_point(outpoint).build()
}

pub fn build_simple_tx(
    input_cells: Vec<CellInput>,
    output_cells: Vec<CellOutput>,
    cell_deps: Vec<CellDep>,
    outputs_data: Vec<packed::Bytes>,
) -> TransactionView {
    TransactionBuilder::default()
        .inputs(input_cells)
        .outputs(output_cells)
        .outputs_data(outputs_data)
        .cell_deps(cell_deps)
        .build()
}

pub fn build_spore_materials(context: &mut Context) -> (OutPoint, CellDep) {
    let spore_bin: Bytes = Loader::default().load_binary("spore");
    let spore_out_point = context.deploy_cell(spore_bin);
    let spore_script_dep = CellDep::new_builder()
        .out_point(spore_out_point.clone())
        .build();
    (spore_out_point, spore_script_dep)
}

pub fn simple_build_context(
    output_data: Vec<u8>,
    content_type: &str,
    input_data: Option<SporeData>,
    out_index: usize,
) -> (Context, TransactionView) {
    let output_data = build_serialized_spore_internal(output_data, content_type, None);
    let capacity = output_data.total_size() as u64;
    let mut context = Context::default();

    // always success lock
    let (spore_out_point, spore_script_dep) = build_spore_materials(&mut context);
    let (input, type_id) = match input_data {
        None => {
            let input = build_normal_input(&mut context, capacity);
            let spore_type_id = build_type_id(&input, out_index);
            (input, spore_type_id)
        }
        Some(input_data) => {
            let input_capacity = input_data.total_size() as u64;
            let spore_type_id =
                build_type_id(&build_normal_input(&mut context, input_capacity), out_index);
            let spore_type = build_spore_type_script(
                &mut context,
                &spore_out_point,
                spore_type_id.to_vec().into(),
            );
            let spore_input = build_spore_input(&mut context, spore_type, input_data);
            (spore_input, spore_type_id)
        }
    };
    let spore_type =
        build_spore_type_script(&mut context, &spore_out_point, type_id.to_vec().into());
    let spore_output = build_output_cell_with_type_id(&mut context, capacity, spore_type.clone());
    let tx = build_simple_tx(
        vec![input],
        vec![spore_output],
        vec![spore_script_dep],
        vec![output_data.as_slice().pack()],
    );

    let action = co_build::build_mint_action(&mut context, type_id, output_data.as_slice());
    let tx = co_build::complete_co_build_message_with_actions(tx, &[(spore_type, action)]);

    (context, tx)
}

pub fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

pub fn calc_code_hash(data: Bytes) -> [u8; 32] {
    let mut blake2b = new_blake2b();
    blake2b.update(data.to_vec().as_slice());
    let mut hash = [0u8; 32];
    blake2b.finalize(&mut hash);
    hash
}

pub fn build_simple_create_context_with_cluster(
    nft_content: String,
    nft_type: String,
    cluster_id: String,
) -> (Context, TransactionView) {
    let nft_data: NativeNFTData = NativeNFTData {
        content: nft_content.clone().into_bytes(),
        content_type: nft_type.clone(),
        cluster_id: Some(
            H256::from_trimmed_str(cluster_id.clone().trim_start_matches("0x"))
                .expect("parse cluster id")
                .as_bytes()
                .to_vec(),
        ),
    };
    let serialized = SporeData::from(nft_data);
    build_create_context_with_cluster_raw(serialized, cluster_id)
}

pub fn build_create_context_with_cluster_raw(
    nft_data: SporeData,
    cluster_id: String,
) -> (Context, TransactionView) {
    let dummy_cluster_name = "Spore Cluster!";
    let dummy_cluster_description = "Spore Description!";

    let cluster_data = ClusterData::new_builder()
        .name(dummy_cluster_name.pack().as_slice().into())
        .description(dummy_cluster_description.pack().as_slice().into())
        .build();
    let mut context = Context::default();
    let nft_bin: Bytes = Loader::default().load_binary("spore");
    let nft_out_point = context.deploy_cell(nft_bin);
    let cluster_bin: Bytes = Loader::default().load_binary("cluster");
    let cluster_out_point = context.deploy_cell(cluster_bin);
    let input_ckb = { nft_data.total_size() } as u64;

    let output_ckb = input_ckb;
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // build lock script
    let lock_script = internal::build_always_success_script(&mut context);
    let lock_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point)
        .build();

    let cluster_script_dep = CellDep::new_builder()
        .out_point(cluster_out_point.clone())
        .build();

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .lock(lock_script.clone())
            .capacity(input_ckb.pack())
            .build(),
        Bytes::new(),
    );

    let cluster_id = H256::from_trimmed_str(cluster_id.clone().trim_start_matches("0x"))
        .expect("parse cluster id")
        .0;

    let cluster_script = context.build_script_with_hash_type(
        &cluster_out_point,
        ScriptHashType::Data1,
        cluster_id.to_vec().into(),
    );

    let cluster_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity((cluster_data.total_size() as u64).pack())
            .lock(lock_script.clone())
            .type_(cluster_script.pack())
            .build(),
        Bytes::copy_from_slice(cluster_data.as_slice()),
    );

    let cluster_dep = CellDep::new_builder()
        .out_point(cluster_out_point.clone())
        .build();

    let cluster_input = CellInput::new_builder()
        .previous_output(cluster_out_point)
        .build();

    let normal_input = CellInput::new_builder()
        .previous_output(
            context.create_cell(
                CellOutput::new_builder()
                    .capacity(1000000u64.pack())
                    .lock(lock_script.clone())
                    .build(),
                Bytes::new(),
            ),
        )
        .build();

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let nft_id = {
        let mut blake2b = Blake2bBuilder::new(32)
            .personal(b"ckb-default-hash")
            .build();
        blake2b.update(input.as_slice());
        blake2b.update(&1u64.to_le_bytes());
        let mut verify_id = [0; 32];
        blake2b.finalize(&mut verify_id);
        verify_id
    };

    let nft_script = context.build_script_with_hash_type(
        &nft_out_point,
        ScriptHashType::Data1,
        nft_id.to_vec().into(),
    );

    let nft_script_dep = CellDep::new_builder().out_point(nft_out_point).build();

    let output = CellOutput::new_builder()
        .capacity((output_ckb + cluster_data.total_size() as u64).pack())
        .lock(lock_script.clone())
        .type_(nft_script.pack())
        .build();

    let cluster_output = CellOutput::new_builder()
        .capacity(input_ckb.pack())
        .lock(lock_script.clone())
        .type_(cluster_script.pack())
        .build();

    let normal_output = CellOutput::new_builder()
        .capacity(9999u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .inputs(vec![input, normal_input, cluster_input])
        .outputs(vec![normal_output, output, cluster_output])
        .outputs_data(vec![
            packed::Bytes::default(),
            nft_data.as_slice().pack(),
            cluster_data.as_slice().pack(),
        ])
        .cell_deps(vec![
            lock_script_dep,
            cluster_script_dep,
            nft_script_dep,
            cluster_dep,
        ])
        .build();

    let cluster_transfer = co_build::build_cluster_transfer_action(&mut context, cluster_id);
    let nft_action = co_build::build_mint_action(&mut context, nft_id, nft_data.as_slice());
    let tx = co_build::complete_co_build_message_with_actions(
        tx,
        &[(cluster_script, cluster_transfer), (nft_script, nft_action)],
    );

    (context, tx)
}
