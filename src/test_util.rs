use std::env;
use std::ffi::{OsStr, OsString};

pub(crate) struct EnvVarGuard {
    name: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(name: &'static str, value: &str) -> Self {
        let previous = env::var_os(name);
        env::set_var(name, value);
        Self { name, previous }
    }

    pub(crate) fn set_os(name: &'static str, value: &OsStr) -> Self {
        let previous = env::var_os(name);
        env::set_var(name, value);
        Self { name, previous }
    }

    pub(crate) fn remove(name: &'static str) -> Self {
        let previous = env::var_os(name);
        env::remove_var(name);
        Self { name, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => env::set_var(self.name, value),
            None => env::remove_var(self.name),
        }
    }
}
