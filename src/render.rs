use std::path::PathBuf;

use anyhow::{Context, Result};
use minijinja::{Environment, path_loader};
use serde::Serialize;

pub struct Templates {
    environment: Environment<'static>,
}

impl Templates {
    pub fn load(path: PathBuf) -> Result<Self> {
        if !path.is_dir() {
            anyhow::bail!("template directory does not exist: {}", path.display());
        }
        let mut environment = Environment::new();
        environment.set_loader(path_loader(path));
        Ok(Self { environment })
    }

    pub fn render<S: Serialize>(&self, name: &str, context: S) -> Result<String> {
        self.environment
            .get_template(name)
            .with_context(|| format!("could not load template {name}"))?
            .render(context)
            .with_context(|| format!("could not render template {name}"))
    }
}
