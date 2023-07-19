#[derive(Debug, Default)]
pub struct CriticalError {
    pub file: &'static str,
    pub line: u32,
}

impl std::fmt::Display for CriticalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "critical level error in {}:{}", self.file, self.line)
    }
}

#[macro_export]
macro_rules! critical_error {
    () => {
        $crate::error::CriticalError {
            line: line!(),
            file: file!(),
        }
    };
}
