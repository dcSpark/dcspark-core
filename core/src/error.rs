#[derive(Debug, Default)]
pub struct CriticalError {}

impl std::fmt::Display for CriticalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "critical level error")
    }
}
