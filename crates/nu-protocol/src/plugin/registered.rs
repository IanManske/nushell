use std::{any::Any, sync::Arc};

use crate::{PluginGcConfig, PluginIdentity, ShellResult};

/// Trait for plugins registered in the [`EngineState`](crate::engine::EngineState).
pub trait RegisteredPlugin: Send + Sync {
    /// The identity of the plugin - its filename, shell, and friendly name.
    fn identity(&self) -> &PluginIdentity;

    /// True if the plugin is currently running.
    fn is_running(&self) -> bool;

    /// Process ID of the plugin executable, if running.
    fn pid(&self) -> Option<u32>;

    /// Set garbage collection config for the plugin.
    fn set_gc_config(&self, gc_config: &PluginGcConfig);

    /// Stop the plugin.
    fn stop(&self) -> ShellResult<()>;

    /// Stop the plugin and reset any state so that we don't make any assumptions about the plugin
    /// next time it launches. This is used on `register`.
    fn reset(&self) -> ShellResult<()>;

    /// Cast the pointer to an [`Any`] so that its concrete type can be retrieved.
    ///
    /// This is necessary in order to allow `nu_plugin` to handle the implementation details of
    /// plugins.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}
