use std::sync::Arc;

use crate::{config::Config, service::LinkService};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<LinkService>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(service: Arc<LinkService>, config: Arc<Config>) -> Self {
        Self { service, config }
    }
}
