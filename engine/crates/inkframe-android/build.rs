use std::env;
use std::fs;
use std::path::PathBuf;

use naga::back::spv;
use naga::front::wgsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::ShaderStage;

fn compile_stage(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: ShaderStage,
    entry_point: &str,
    output_name: &str,
) -> Result<(), String> {
    // Vulkan clip-space coordinates are authored directly by the shader, so do
    // not ask Naga to apply its optional coordinate-space adjustment.
    let options = spv::Options {
        lang_version: (1, 0),
        flags: spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    let pipeline = spv::PipelineOptions {
        shader_stage: stage,
        entry_point: entry_point.to_string(),
    };
    let words = spv::write_vec(module, info, &options, Some(&pipeline))
        .map_err(|error| format!("SPIR-V generation for {entry_point} failed: {error}"))?;

    let mut bytes = Vec::with_capacity(words.len() * 4);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    fs::write(out_dir.join(output_name), bytes)
        .map_err(|error| format!("writing {output_name} failed: {error}"))
}

fn main() -> Result<(), String> {
    const SHADER_PATH: &str = "shaders/brush.wgsl";
    println!("cargo:rerun-if-changed={SHADER_PATH}");

    let source = fs::read_to_string(SHADER_PATH)
        .map_err(|error| format!("reading {SHADER_PATH} failed: {error}"))?;
    let module = wgsl::parse_str(&source)
        .map_err(|error| format!("parsing {SHADER_PATH} failed: {}", error.emit_to_string(&source)))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| format!("validating {SHADER_PATH} failed: {error}"))?;

    compile_stage(
        &module,
        &info,
        ShaderStage::Vertex,
        "vs_main",
        "brush.vert.spv",
    )?;
    compile_stage(
        &module,
        &info,
        ShaderStage::Fragment,
        "fs_main",
        "brush.frag.spv",
    )?;
    Ok(())
}
