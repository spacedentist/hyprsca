use crate::types::Head;

pub trait Backend {
    fn get_all_heads(&self) -> anyhow::Result<Vec<Head>>;
    fn set_head_config(&self, heads: &[Head]) -> anyhow::Result<()>;
    fn fallback_head_config(
        &self,
        active_head_names: &[String],
        inactive_head_names: &[String],
    ) -> anyhow::Result<()>;
}

mod hyprctl;
pub use hyprctl::HyprctlBackend;
mod wlr_randr;
pub use wlr_randr::WlrRandrBackend;
