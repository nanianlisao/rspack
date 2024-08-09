use napi::Either;
use rspack_core::{
  LoaderContext, NormalModuleLoaderShouldYield, NormalModuleLoaderStartYielding, RunnerContext,
  BUILTIN_LOADER_PREFIX,
};
use rspack_error::{error, Result};
use rspack_hook::plugin_hook;
use rspack_loader_runner::State as LoaderState;

use super::{
  JsLoaderContext, JsLoaderContextMethodsWrapper, JsLoaderRspackPlugin, JsLoaderRspackPluginInner,
};

#[plugin_hook(NormalModuleLoaderShouldYield for JsLoaderRspackPlugin)]
pub(crate) fn loader_should_yield(
  &self,
  loader_context: &LoaderContext<RunnerContext>,
) -> Result<Option<bool>> {
  match loader_context.state() {
    s @ LoaderState::Init | s @ LoaderState::ProcessResource | s @ LoaderState::Finished => {
      panic!("Unexpected loader runner state: {s:?}")
    }
    LoaderState::Pitching | LoaderState::Normal => {
      return Ok(Some(
        !loader_context
          .current_loader()
          .request()
          .starts_with(BUILTIN_LOADER_PREFIX),
      ))
    }
  }
}

#[plugin_hook(NormalModuleLoaderStartYielding for JsLoaderRspackPlugin)]
pub(crate) async fn loader_yield(
  &self,
  loader_context: &mut LoaderContext<RunnerContext>,
) -> Result<()> {
  let js_loader_context = JsLoaderContext::new(loader_context)?;
  let js_loader_context_methods = JsLoaderContextMethodsWrapper::new(loader_context);
  let new_cx = self
    .runner
    .call_with_promise((js_loader_context, js_loader_context_methods))
    .await?;
  merge_loader_context(loader_context, new_cx)?;
  Ok(())
}

fn merge_loader_context(
  to: &mut LoaderContext<RunnerContext>,
  mut from: JsLoaderContext,
) -> Result<()> {
  if let Some(data) = &from.additional_data {
    to.additional_data.insert(data.clone());
  }
  to.content = match from.content {
    Either::A(_) => None,
    Either::B(c) => Some(rspack_core::Content::from(Into::<Vec<u8>>::into(c))),
  };
  to.source_map = from
    .source_map
    .as_ref()
    .map(|s| rspack_core::rspack_sources::SourceMap::from_slice(s))
    .transpose()
    .map_err(|e| error!(e.to_string()))?;

  // update loader status
  to.loader_items = to
    .loader_items
    .drain(..)
    .zip(from.loader_items.drain(..))
    .map(|(mut to, from)| {
      if from.normal_executed {
        to.set_normal_executed()
      }
      if from.pitch_executed {
        to.set_pitch_executed()
      }
      to.set_data(from.data);
      to
    })
    .collect();
  to.loader_index = from.loader_index;
  Ok(())
}
