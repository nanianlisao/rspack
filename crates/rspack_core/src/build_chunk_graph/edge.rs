use num_bigint::BigUint;
use rspack_collections::UkeySet;
use rspack_error::Result;

use super::code_splitter::{CgiUkey, CodeSplitter};
use crate::{
  AsyncDependenciesBlockIdentifier, ChunkGroupUkey, ChunkUkey, Compilation, ModuleIdentifier,
};

#[derive(Debug, Clone)]
pub enum IterateBlockEdge {
  Entry(String),
  Block(BlockEdge),
}

#[derive(Debug, Clone)]
pub struct BlockEdge {
  pub block_id: AsyncDependenciesBlockIdentifier,
  pub module_id: ModuleIdentifier,
  pub item_chunk_group_info_ukey: CgiUkey,
  pub item_chunk_ukey: ChunkUkey,
  pub available_modules: BigUint,
  pub target_chunk_group_info_ukey: CgiUkey,
}

impl CodeSplitter {
  pub(crate) fn invalidate_from_module(
    &mut self,
    module: ModuleIdentifier,
    compilation: &mut Compilation,
  ) -> Result<Vec<IterateBlockEdge>> {
    let chunk_graph = &mut compilation.chunk_graph;

    // Step 1. find all invalidate chunk groups and remove module from ChunkGraph
    let Some(cgm) = chunk_graph.get_chunk_graph_module_mut(module) else {
      return Ok(vec![]);
    };

    let invalidate_chunk_groups = cgm
      .chunks
      .iter()
      .map(|chunk| {
        let chunk = compilation.chunk_by_ukey.expect_get(chunk);
        chunk.groups.clone()
      })
      .flatten()
      .collect::<UkeySet<ChunkGroupUkey>>();

    // chunk_graph.remove_module(module);

    // Step 2. remove edges, and prepare to recalculate edges
    let mut re_calc_edges = Vec::with_capacity(invalidate_chunk_groups.len());
    for chunk_group_ukey in &invalidate_chunk_groups {
      if let Some(edge) = self.invalidate_chunk_group(*chunk_group_ukey, compilation) {
        re_calc_edges.push(edge);
      }
    }

    Ok(re_calc_edges)
  }

  pub(crate) fn invalidate_chunk_group(
    &mut self,
    chunk_group_ukey: ChunkGroupUkey,
    compilation: &mut Compilation,
  ) -> Option<IterateBlockEdge> {
    // prepare data
    let Some(cgi_ukey) = self.chunk_group_info_map.remove(&chunk_group_ukey) else {
      return None;
    };
    let Some(chunk_group_info) = self.chunk_group_infos.remove(&cgi_ukey) else {
      return None;
    };
    let Some(chunk_group) = compilation.chunk_group_by_ukey.remove(&chunk_group_ukey) else {
      return None;
    };

    let chunk_group_name = chunk_group.name().map(|s| s.to_string());
    if let Some(name) = &chunk_group_name {
      compilation.named_chunk_groups.remove(name);
      compilation.entrypoints.swap_remove(name);
    }

    // remove child parent relations
    for child in chunk_group_info.children.iter() {
      let Some(child_cgi) = self.chunk_group_infos.get_mut(child) else {
        continue;
      };

      child_cgi.available_sources.swap_remove(&cgi_ukey);

      if let Some(child_cg) = compilation
        .chunk_group_by_ukey
        .get_mut(&child_cgi.chunk_group)
      {
        child_cg.parents.remove(&chunk_group_ukey);
      }
    }

    for parent in chunk_group.parents.iter() {
      let Some(parent_cg) = compilation.chunk_group_by_ukey.get_mut(parent) else {
        continue;
      };

      parent_cg.children.remove(&chunk_group_ukey);

      if let Some(parent_cgi) = self.chunk_group_info_map.get(parent) {
        if let Some(parent_cgi) = self.chunk_group_infos.get_mut(parent_cgi) {
          parent_cgi.children.swap_remove(&cgi_ukey);
          parent_cgi.available_children.swap_remove(&cgi_ukey);
        }
      }
    }

    let chunk_graph = &mut compilation.chunk_graph;
    // remove cgc and cgm
    for chunk_ukey in chunk_group.chunks {
      if let Some(chunk_graph_chunk) = chunk_graph.remove_chunk_graph_chunk(&chunk_ukey) {
        for module_identifier in chunk_graph_chunk.modules {
          let Some(cgm) = chunk_graph.get_chunk_graph_module_mut(module_identifier) else {
            continue;
          };

          if cgm.chunks.remove(&chunk_ukey) && cgm.chunks.is_empty() {
            chunk_graph.remove_module(module_identifier)
          }
        }
      };

      let Some(chunk) = compilation.chunk_by_ukey.get_mut(&chunk_ukey) else {
        continue;
      };

      if chunk.groups.remove(&chunk_group_ukey) && chunk.groups.is_empty() {
        // remove orphan chunk
        if let Some(name) = &chunk.name {
          compilation.named_chunks.remove(name);
        }
        compilation.chunk_by_ukey.remove(&chunk_ukey);
      }
    }

    // remove chunk group
    compilation.chunk_group_by_ukey.remove(&chunk_group_ukey);

    // remove runtime chunk
    if let Some(runtime_chunk) = chunk_group.runtime_chunk {
      self.runtime_chunks.remove(&runtime_chunk);
    }

    // remove data related to cgi
    self.block_by_cgi.remove(&cgi_ukey);
    if let Some(block_id) = &chunk_group_info.block_id {
      self.block_edges.remove(&block_id.as_identifier());
    }

    if let Some(block_id) = chunk_group_info.block_id {
      self.block_chunk_groups.remove(&block_id);
      self.block_edges.remove(&block_id.as_identifier())
    } else {
      Some(IterateBlockEdge::Entry(
        chunk_group_name
          .expect("entrypoint should have name")
          .into(),
      ))
    }
  }

  pub(crate) fn remove_orphan(&mut self, compilation: &mut Compilation) {
    let mut removed = vec![];
    for chunk_group in compilation.chunk_group_by_ukey.values() {
      let ukey = chunk_group.ukey;
      if !chunk_group.kind.is_entrypoint() && chunk_group.parents.is_empty() {
        removed.push(ukey);
      }
    }

    for removed_cg in &removed {
      dbg!("remove", removed_cg);
      self.invalidate_chunk_group(*removed_cg, compilation);
    }

    if !removed.is_empty() {
      self.remove_orphan(compilation);
    }
  }
}
