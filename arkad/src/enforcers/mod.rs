pub mod dns;
pub mod hostname;
pub mod ipv6;
pub mod mac;

pub trait Enforcer: Send + Sync {
    fn name(&self) -> &'static str;
    fn enforce(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn verify(&self) -> Result<bool, Box<dyn std::error::Error>>;
}
