//! `require("./name")` for config modules: relative to the requiring file, or to the config dir from evals
//! (`=input`). Each module runs in its own environment (reading through to the shared one, like Roblox scripts)
//! and shares what it returns; Luau caches it per state.

use mlua::{
    Lua,
    luau::{NavigateError, Require},
};
use std::path::PathBuf;

/// The chunk name of evals, which `require` from the config dir
pub(crate) const EVAL_CHUNK: &str = "=input";

/// Where `require` is: a module's path without its extension (`<dir>/init` for `<dir>/init.luau`)
#[derive(Default)]
pub(crate) struct ConfigRequirer {
    path: PathBuf,
}

impl ConfigRequirer {
    /// The module's file: `<path>.luau`/`.lua`, or `<path>/init.luau`
    fn file(&self) -> Option<PathBuf> {
        let with_extension = |extension: &str| {
            let mut file = self.path.clone().into_os_string();
            file.push(extension);
            PathBuf::from(file)
        };
        [
            with_extension(".luau"),
            with_extension(".lua"),
            self.path.join("init.luau"),
        ]
        .into_iter()
        .find(|file| file.is_file())
    }
}

/// A table of its own globals, reading through to the shared ones
pub(crate) fn own_environment(lua: &Lua) -> mlua::Result<mlua::Table> {
    let environment = lua.create_table()?;
    let shared = lua.create_table()?;
    shared.set("__index", lua.globals())?;
    environment.set_metatable(Some(shared))?;
    Ok(environment)
}

impl Require for ConfigRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name == EVAL_CHUNK || chunk_name.starts_with('@')
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        self.path = if chunk_name == EVAL_CHUNK {
            // as if a file in the config dir
            sonamu_common::settings::config_dir()
                .ok_or(NavigateError::NotFound)?
                .join("input")
        } else {
            let file = PathBuf::from(
                chunk_name
                    .strip_prefix('@')
                    .ok_or(NavigateError::NotFound)?,
            );
            file.with_extension("")
        };
        Ok(())
    }

    // no `.luaurc` aliases
    fn jump_to_alias(&mut self, _path: &str) -> Result<(), NavigateError> {
        Err(NavigateError::NotFound)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        if self.path.pop() {
            Ok(())
        } else {
            Err(NavigateError::NotFound)
        }
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        self.path.push(name);
        Ok(())
    }

    fn has_module(&self) -> bool {
        self.file().is_some()
    }

    fn cache_key(&self) -> String {
        self.file().unwrap_or_default().display().to_string()
    }

    fn has_config(&self) -> bool {
        false
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::NotFound.into())
    }

    fn loader(&self, lua: &Lua) -> mlua::Result<mlua::Function> {
        let file = self
            .file()
            .ok_or_else(|| mlua::Error::runtime("the module is gone"))?;
        lua.load(file.as_path())
            .set_name(format!("@{}", file.display()))
            .set_environment(own_environment(lua)?)
            .into_function()
    }
}
