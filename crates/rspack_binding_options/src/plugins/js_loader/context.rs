use std::cell::RefCell;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use rspack_binding_values::{JsModule, JsResourceData, ToJsModule};
use rspack_core::{LoaderContext, LoaderContextId, RunnerContext};
use rspack_error::{error, Result};
use rspack_loader_runner::{LoaderItem, State as LoaderState};
use rspack_napi::{threadsafe_js_value_ref::ThreadsafeJsValueRef, Ref};
use rustc_hash::FxHashMap as HashMap;

#[napi(object)]
pub struct JsLoaderItem {
  pub request: String,
  pub r#type: String,

  // data
  pub data: serde_json::Value,

  // status
  pub normal_executed: bool,
  pub pitch_executed: bool,
}

impl From<&LoaderItem<RunnerContext>> for JsLoaderItem {
  fn from(value: &LoaderItem<RunnerContext>) -> Self {
    JsLoaderItem {
      request: value.request().to_string(),
      r#type: value.r#type().to_string(),

      data: value.data().clone(),
      normal_executed: value.normal_executed(),
      pitch_executed: value.pitch_executed(),
    }
  }
}

#[napi(string_enum)]
pub enum JsLoaderState {
  Pitching,
  Normal,
}

impl From<LoaderState> for JsLoaderState {
  fn from(value: LoaderState) -> Self {
    match value {
      LoaderState::Init | LoaderState::ProcessResource | LoaderState::Finished => {
        panic!("Unexpected loader runner state: {value:?}")
      }
      LoaderState::Pitching => JsLoaderState::Pitching,
      LoaderState::Normal => JsLoaderState::Normal,
    }
  }
}

#[napi(object)]
pub struct JsLoaderContext {
  pub loader_index: i32,
  pub resource_data: JsResourceData,
  #[napi(js_name = "_moduleIdentifier")]
  pub module_identifier: String,
  #[napi(js_name = "_module")]
  pub module: JsModule,
  pub hot: bool,
  pub content: Either<Null, Buffer>,
  #[napi(ts_type = "any")]
  pub additional_data: Option<ThreadsafeJsValueRef<Unknown>>,
  pub source_map: Option<Buffer>,
  pub loader_items: Vec<JsLoaderItem>,
  pub loader_state: JsLoaderState,
}

impl JsLoaderContext {
  pub fn new(ctx: &LoaderContext<RunnerContext>) -> Result<Self> {
    Ok(Self {
      loader_index: ctx.loader_index,
      resource_data: ctx.resource_data.as_ref().into(),
      module_identifier: ctx.context.module.module_identifier.to_string(),
      module: ctx
        .context
        .module
        .to_js_module()
        .expect("CompilerModuleContext::to_js_module should not fail."),
      hot: ctx.hot,
      content: match &ctx.content {
        Some(c) => Either::B(c.to_owned().into_bytes().into()),
        None => Either::A(Null),
      },
      additional_data: ctx
        .additional_data
        .get::<ThreadsafeJsValueRef<Unknown>>()
        .cloned(),
      source_map: ctx
        .source_map
        .clone()
        .map(|v| v.to_json())
        .transpose()
        .map_err(|e| error!(e.to_string()))?
        .map(|v| v.into_bytes().into()),
      loader_items: ctx.loader_items.iter().map(Into::into).collect(),
      loader_state: ctx.state().into(),
    })
  }
}

#[napi]
pub struct JsLoaderContextMethods(pub(crate) &'static mut LoaderContext<RunnerContext>);

#[napi]
impl JsLoaderContextMethods {
  #[napi]
  pub fn cacheable(&mut self, val: bool) {
    if !val {
      self.0.cacheable = val;
    }
  }

  #[napi]
  pub fn add_dependency(&mut self, file: String) {
    self.0.file_dependencies.insert(file.into());
  }

  #[napi]
  pub fn add_context_dependency(&mut self, file: String) {
    self.0.context_dependencies.insert(file.into());
  }

  #[napi]
  pub fn add_missing_dependency(&mut self, file: String) {
    self.0.missing_dependencies.insert(file.into());
  }

  #[napi]
  pub fn add_build_dependency(&mut self, file: String) {
    self.0.build_dependencies.insert(file.into());
  }

  #[napi]
  pub fn get_dependencies(&self) -> Vec<String> {
    self
      .0
      .file_dependencies
      .iter()
      .map(|i| i.to_string_lossy().to_string())
      .collect()
  }

  #[napi]
  pub fn get_context_dependencies(&self) -> Vec<String> {
    self
      .0
      .context_dependencies
      .iter()
      .map(|i| i.to_string_lossy().to_string())
      .collect()
  }

  #[napi]
  pub fn get_missing_dependencies(&self) -> Vec<String> {
    self
      .0
      .missing_dependencies
      .iter()
      .map(|i| i.to_string_lossy().to_string())
      .collect()
  }

  #[napi]
  pub fn clear_dependencies(&mut self) {
    self.0.file_dependencies.clear();
    self.0.context_dependencies.clear();
    self.0.build_dependencies.clear();
    self.0.cacheable = true;
  }
}

thread_local! {
  pub static LOADER_CONTEXT_INSTANCE_REFS: RefCell<HashMap<LoaderContextId, Ref>> = Default::default();
}

pub struct JsLoaderContextMethodsWrapper(&'static mut LoaderContext<RunnerContext>);

impl JsLoaderContextMethodsWrapper {
  pub fn new(value: &mut LoaderContext<RunnerContext>) -> Self {
    let context = unsafe {
      std::mem::transmute::<
        &'_ mut LoaderContext<RunnerContext>,
        &'static mut LoaderContext<RunnerContext>,
      >(value)
    };
    Self(context)
  }
}

impl ToNapiValue for JsLoaderContextMethodsWrapper {
  unsafe fn to_napi_value(env: sys::napi_env, val: Self) -> napi::Result<sys::napi_value> {
    LOADER_CONTEXT_INSTANCE_REFS.with(|refs| {
      let mut refs = refs.borrow_mut();
      match refs.entry(val.0.id) {
        std::collections::hash_map::Entry::Occupied(entry) => {
          let r = entry.get();
          ToNapiValue::to_napi_value(env, r)
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
          let env_wrapper = Env::from_raw(env);
          let instance = JsLoaderContextMethods(val.0).into_instance(env_wrapper)?;
          let napi_value = ToNapiValue::to_napi_value(env, instance)?;
          let r = Ref::new(env, napi_value, 1)?;
          let r = entry.insert(r);
          ToNapiValue::to_napi_value(env, r)
        }
      }
    })
  }
}
