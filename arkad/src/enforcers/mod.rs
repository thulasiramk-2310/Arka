pub mod dns;
pub mod hostname;
pub mod ipv6;
pub mod mac;
pub mod sandbox;

use crate::error::ArkadError;
use crate::state::SharedState;

#[async_trait::async_trait]
pub trait AsyncEnforcer: Send + Sync {
    fn name(&self) -> &'static str;
    async fn enforce(&self) -> Result<(), ArkadError>;
    async fn update_state(&self, state: &SharedState);
}
