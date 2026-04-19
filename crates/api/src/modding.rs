use serde::{Deserialize, Serialize};

/// Shared metadata that identifies one mod package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// Minimal event hook surface for community extensions.
pub trait ModEvent: Send + Sync + 'static {
    fn name(&self) -> &'static str;
}

/// Read-only registry access exposed to mods.
pub trait RegistryAccess {
    fn has_block(&self, name: &str) -> bool;
    fn has_item(&self, name: &str) -> bool;
}

/// Capability lookup surface for mods.
pub trait CapabilityAccess {
    fn has_capability(&self, name: &str) -> bool;
}

/// Shared context passed to client and server mod entrypoints.
pub trait ModContext: RegistryAccess + CapabilityAccess {
    fn metadata(&self) -> &ModMetadata;
}

/// Server-side mod entrypoint.
pub trait ServerMod {
    fn metadata(&self) -> ModMetadata;
    fn on_load(&mut self, _context: &mut dyn ModContext) {}
}

/// Client-side mod entrypoint.
pub trait ClientMod {
    fn metadata(&self) -> ModMetadata;
    fn on_load(&mut self, _context: &mut dyn ModContext) {}
}
