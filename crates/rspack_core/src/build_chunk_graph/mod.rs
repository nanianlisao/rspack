// use rspack_core::Bundle;
// use rspack_core::ChunkGraph;

use tracing::instrument;

use crate::Compilation;

pub(crate) mod code_splitter;
pub(crate) mod edge;

#[instrument(skip_all)]
pub(crate) fn build_chunk_graph(compilation: &mut Compilation) -> rspack_error::Result<()> {
  // let mut splitter = code_splitter::CodeSplitter::new(compilation);
  let mut splitter = compilation.code_splitting_cache.code_splitter.clone();
  splitter.update_with_compilation(compilation)?;
  dbg!("update");
  if splitter.chunk_group_infos.is_empty() {
    let inputs = splitter.prepare_input_entrypoints_and_modules(compilation)?;
    dbg!("prepare entrypoints and modules");

    splitter.prepare_entries(inputs, compilation)?;
    dbg!("prepare entries");
  }

  splitter.split(compilation)?;
  dbg!("splitted");

  splitter.remove_orphan(compilation);
  dbg!("removed orphan");

  // make sure all module (weak dependency particularly) has a cgm
  let ids = compilation
    .get_module_graph()
    .modules()
    .keys()
    .copied()
    .collect::<Vec<_>>();

  for module_identifier in ids {
    compilation.chunk_graph.add_module(module_identifier)
  }

  compilation.code_splitting_cache.code_splitter = splitter;

  Ok(())
}
