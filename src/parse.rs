use rowan::{ast::AstNode, GreenNode, SyntaxNode};
use std::marker::PhantomData;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parse result containing a syntax tree and any parse errors
pub struct Parse<T> {
    green: GreenNode,
    errors: Vec<String>,
    positioned_errors: Vec<crate::edit::lossless::PositionedParseError>,
    _ty: PhantomData<T>,
}

impl<T> Parse<T> {
    /// Create a new parse result
    pub fn new(green: GreenNode, errors: Vec<String>) -> Self {
        Parse {
            green,
            errors,
            positioned_errors: Vec::new(),
            _ty: PhantomData,
        }
    }

    /// Create a new parse result with positioned errors
    pub fn new_with_positioned_errors(
        green: GreenNode,
        errors: Vec<String>,
        positioned_errors: Vec<crate::edit::lossless::PositionedParseError>,
    ) -> Self {
        Parse {
            green,
            errors,
            positioned_errors,
            _ty: PhantomData,
        }
    }

    /// Get the green node (thread-safe representation)
    pub fn green(&self) -> &GreenNode {
        &self.green
    }

    /// Get the syntax errors
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Get parse errors with position information
    pub fn positioned_errors(&self) -> &[crate::edit::lossless::PositionedParseError] {
        &self.positioned_errors
    }

    /// Get parse errors as strings
    pub fn error_messages(&self) -> Vec<String> {
        self.positioned_errors
            .iter()
            .map(|e| e.message.clone())
            .collect()
    }

    /// Check if parsing succeeded without errors
    pub fn ok(&self) -> bool {
        self.errors.is_empty() && self.positioned_errors.is_empty()
    }

    /// Convert to a Result, returning the tree if there are no errors
    pub fn to_result(self) -> Result<T, crate::edit::lossless::ParseError>
    where
        T: AstNode<Language = crate::edit::lossless::Lang>,
    {
        if self.errors.is_empty() && self.positioned_errors.is_empty() {
            let node = SyntaxNode::<crate::edit::lossless::Lang>::new_root(self.green);
            Ok(T::cast(node).expect("root node has wrong type"))
        } else {
            let mut all_errors = self.errors.clone();
            all_errors.extend(self.error_messages());
            Err(crate::edit::lossless::ParseError(all_errors))
        }
    }

    /// Get the parsed syntax tree, panicking if there are errors
    pub fn tree(&self) -> T
    where
        T: AstNode<Language = crate::edit::lossless::Lang>,
    {
        assert!(
            self.errors.is_empty() && self.positioned_errors.is_empty(),
            "tried to get tree with errors: {:?}",
            self.errors
        );
        let node = SyntaxNode::<crate::edit::lossless::Lang>::new_root(self.green.clone());
        T::cast(node).expect("root node has wrong type")
    }

    /// Get the syntax node
    pub fn syntax_node(&self) -> SyntaxNode<crate::edit::lossless::Lang> {
        SyntaxNode::<crate::edit::lossless::Lang>::new_root(self.green.clone())
    }

    /// Cast this parse result to a different AST node type
    pub fn cast<U>(self) -> Option<Parse<U>>
    where
        U: AstNode<Language = crate::edit::lossless::Lang>,
    {
        let node = SyntaxNode::<crate::edit::lossless::Lang>::new_root(self.green.clone());
        U::cast(node)?;
        Some(Parse {
            green: self.green,
            errors: self.errors,
            positioned_errors: self.positioned_errors,
            _ty: PhantomData,
        })
    }
}
