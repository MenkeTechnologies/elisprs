//! Error / signal representation.
//!
//! Elisp signals carry an error *symbol* (`void-variable`,
//! `wrong-type-argument`, …) plus data. Milestone 1 renders the data to a
//! string; milestone 2 will carry a real `Value` list so `condition-case` can
//! destructure `(error-symbol . data)`.

#[derive(Debug, Clone)]
pub struct ElError {
    pub symbol: String,
    pub data: String,
}

impl ElError {
    pub fn new(symbol: &str, data: impl Into<String>) -> Self {
        ElError { symbol: symbol.to_string(), data: data.into() }
    }
    pub fn err(data: impl Into<String>) -> Self {
        ElError::new("error", data)
    }
    pub fn void_variable(name: &str) -> Self {
        ElError::new("void-variable", format!("Symbol's value as variable is void: {name}"))
    }
    pub fn void_function(name: &str) -> Self {
        ElError::new("void-function", format!("Symbol's function definition is void: {name}"))
    }
    pub fn wrong_type(expected: &str, got: &str) -> Self {
        ElError::new("wrong-type-argument", format!("expected {expected}, got {got}"))
    }
    pub fn wrong_args(name: &str) -> Self {
        ElError::new("wrong-number-of-arguments", format!("wrong number of arguments: {name}"))
    }
}

impl std::fmt::Display for ElError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.symbol, self.data)
    }
}

pub type ElResult<T> = Result<T, ElError>;
