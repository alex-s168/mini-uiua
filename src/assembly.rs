use std::{
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
    ops::{Index, IndexMut},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    collections::HashMap,
};

use ecow::{eco_vec, EcoString, EcoVec};

use crate::{
    compile::{LocalName, Module},
    is_ident_char, CodeSpan, FunctionId, InputSrc, IntoInputSrc, Node, SigNode, Signature, Span,
    Uiua, UiuaResult, Value,
};

/// A compiled Uiua assembly
#[derive(Clone)]
pub struct Assembly {
    /// The top-level node
    pub root: Node,
    /// Functions
    pub(crate) functions: EcoVec<Node>,
    /// A list of global bindings
    pub bindings: EcoVec<BindingInfo>,
    /// A list of data definitions
    pub defs: EcoVec<DefInfo>,
    pub(crate) spans: EcoVec<Span>,
    /// Inputs used to build the assembly
    pub inputs: Inputs,
    pub(crate) dynamic_functions: EcoVec<DynFn>,
    pub(crate) test_assert_count: usize,
}

/// A Uiua function
///
/// This does not actually contain the function's code.
/// It is a lightweight handle that can be used to look up the function's code in an [`Assembly`].
///
/// It also contains the function's [`FunctionId`] and [`Signature`].
#[derive(Clone)]
pub struct Function {
    /// The function's id
    pub id: FunctionId,
    /// The function's signature
    pub sig: Signature,
    pub(crate) index: usize,
    hash: u64,
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ← {}", self.id, self.sig)
    }
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.sig == other.sig && self.hash == other.hash
    }
}

impl Eq for Function {}

impl Hash for Function {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

/// Information for a data definition
#[derive(Debug, Clone)]
pub struct DefInfo {
    /// The name of the definition
    pub name: Option<EcoString>,
}

impl Assembly {
    /// Get the [`SigNode`] for a function
    pub fn sig_node(&self, f: &Function) -> SigNode {
        SigNode::new(f.sig, self[f].clone())
    }
    /// Add a function to the assembly
    pub fn add_function(&mut self, id: FunctionId, sig: Signature, mut root: Node) -> Function {
        root.optimize_early();
        let mut hasher = DefaultHasher::new();
        root.hash(&mut hasher);
        let hash = hasher.finish();
        self.functions.push(root);
        let index = self.functions.len() - 1;
        Function {
            id,
            sig,
            index,
            hash,
        }
    }
    pub(crate) fn add_binding_at(
        &mut self,
        local: LocalName,
        global: BindingKind,
        span: Option<CodeSpan>,
        meta: BindingMeta,
    ) {
        let binding = BindingInfo {
            public: local.public,
            kind: global,
            span: span.unwrap_or_else(CodeSpan::dummy),
            meta,
        };
        if local.index < self.bindings.len() {
            self.bindings.make_mut()[local.index] = binding;
        } else {
            while self.bindings.len() < local.index {
                self.bindings.push(BindingInfo {
                    kind: BindingKind::Const(None),
                    public: false,
                    span: CodeSpan::dummy(),
                    meta: BindingMeta::default(),
                });
            }
            self.bindings.push(binding);
        }
    }
    pub(crate) fn bind_const(
        &mut self,
        local: LocalName,
        value: Option<Value>,
        span: usize,
        meta: BindingMeta,
    ) {
        let span = self.spans[span].clone();
        self.add_binding_at(local, BindingKind::Const(value), span.code(), meta);
    }
    pub(crate) fn bind_def(&mut self, info: DefInfo) -> usize {
        let index = self.defs.len();
        self.defs.push(info);
        index
    }
    #[track_caller]
    pub(crate) fn def(&self, index: usize) -> &DefInfo {
        &self.defs[index]
    }
    /// Parse a `.uasm` file into an assembly
    pub fn from_uasm(src: &str) -> Result<Self, String> {
        panic!("no");
    }
    /// Serialize the assembly into a `.uasm` file
    pub fn to_uasm(&self) -> String {
        panic!("no");
    }
}

impl Index<&Function> for Assembly {
    type Output = Node;
    #[track_caller]
    fn index(&self, func: &Function) -> &Self::Output {
        match self.functions.get(func.index) {
            Some(node) => node,
            None => panic!("{}({:?}) not found in assembly", func.id, func.index),
        }
    }
}

impl IndexMut<&Function> for Assembly {
    #[track_caller]
    fn index_mut(&mut self, func: &Function) -> &mut Self::Output {
        match self.functions.make_mut().get_mut(func.index) {
            Some(node) => node,
            None => panic!("{}({:?}) not found in assembly", func.id, func.index),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
type DynFn = Arc<dyn Fn(&mut Uiua) -> UiuaResult + Send + Sync + 'static>;
#[cfg(target_arch = "wasm32")]
type DynFn = Arc<dyn Fn(&mut Uiua) -> UiuaResult + 'static>;

impl Default for Assembly {
    fn default() -> Self {
        Self {
            root: Node::default(),
            functions: EcoVec::new(),
            defs: EcoVec::new(),
            spans: eco_vec![Span::Builtin],
            bindings: EcoVec::new(),
            dynamic_functions: EcoVec::new(),
            inputs: Inputs::default(),
            test_assert_count: 0,
        }
    }
}

impl From<&Assembly> for Assembly {
    fn from(asm: &Assembly) -> Self {
        asm.clone()
    }
}

/// Information about a binding
#[derive(Debug, Clone)]
pub struct BindingInfo {
    /// The binding kind
    pub kind: BindingKind,
    /// Whether the binding is public
    pub public: bool,
    /// The span of the original binding name
    pub span: CodeSpan,
    /// Metadata about the binding
    pub meta: BindingMeta,
}

/// Metadata about a binding
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindingMeta {
    /// The comment preceding the binding
    pub comment: Option<DocComment>,
    /// The character counts for golfing
    pub counts: Option<BindingCounts>,
    /// The deprecation message
    pub deprecation: Option<EcoString>,
    /// Whether this binding's code was externally provided
    pub external: bool,
}

/// A kind of global binding
#[derive(Debug, Clone)]
pub enum BindingKind {
    /// A constant value
    Const(Option<Value>),
    /// A function
    Func(Function),
    /// An imported module
    Import(PathBuf),
    /// A scoped module
    Module(Module),
    /// A scope being compiled
    Scope(usize),
    /// An index macro
    ///
    /// Contains the number of arguments
    IndexMacro(usize),
    /// A code macro
    CodeMacro(Node),
    /// An error
    Error,
}

impl BindingKind {
    /// Get the signature of the binding
    pub fn sig(&self) -> Option<Signature> {
        match self {
            Self::Const(_) => Some(Signature::new(0, 1)),
            Self::Func(func) => Some(func.sig),
            Self::Import { .. } => None,
            Self::Module(_) => None,
            Self::Scope(_) => None,
            Self::IndexMacro(_) => None,
            Self::CodeMacro(_) => None,
            Self::Error => None,
        }
    }
    /// Check if the global is a once-bound constant
    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Const(_))
    }
}

/// Character counts for a binding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingCounts {
    /// The number of characters
    pub char: usize,
    /// The number of SBCS bytes
    pub sbcs: usize,
}

impl fmt::Display for BindingCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} character{}",
            self.char,
            if self.char == 1 { "" } else { "s" }
        )?;
        if self.sbcs != self.char {
            write!(f, " ({} SBCS)", self.sbcs)?;
        }
        Ok(())
    }
}

/// A comment that documents a binding
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocComment {
    /// The comment text
    pub text: EcoString,
    /// The signature of the binding
    pub sig: Option<DocCommentSig>,
}

/// A signature in a doc comment
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocCommentSig {
    /// Whether this is a labelling signature
    pub label: bool,
    /// The arguments of the signature
    pub args: Option<Vec<DocCommentArg>>,
    /// The outputs of the signature
    pub outputs: Option<Vec<DocCommentArg>>,
}

impl DocCommentSig {
    /// Whether the doc comment signature matches a given function signature
    pub fn matches_sig(&self, sig: Signature) -> bool {
        (self.args.as_ref()).map_or(true, |args| args.len() == sig.args)
            && (self.outputs.as_ref()).map_or(true, |o| o.len() == sig.outputs)
    }
    pub(crate) fn sig_string(&self) -> String {
        match (&self.args, &self.outputs) {
            (Some(args), Some(outputs)) => {
                format!("signature {}", Signature::new(args.len(), outputs.len()))
            }
            (Some(args), None) => format!(
                "{} arg{}",
                args.len(),
                if args.len() == 1 { "" } else { "s" }
            ),
            (None, Some(outputs)) => format!(
                "{} output{}",
                outputs.len(),
                if outputs.len() == 1 { "" } else { "s" }
            ),
            (None, None) => "signature".into(),
        }
    }
}

impl fmt::Display for DocCommentSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(outputs) = &self.outputs {
            for output in outputs {
                write!(f, " {}", output.name)?;
                if let Some(ty) = &output.ty {
                    write!(f, ":{}", ty)?;
                }
            }
            write!(f, " ")?;
        }
        if self.label {
            write!(f, "$")?;
        } else {
            write!(f, "?")?;
        }
        if let Some(args) = &self.args {
            write!(f, " ")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}", arg.name)?;
                if let Some(ty) = &arg.ty {
                    write!(f, ":{}", ty)?;
                }
            }
        }
        Ok(())
    }
}

/// An argument in a doc comment signature
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocCommentArg {
    /// The name of the argument
    pub name: EcoString,
    /// A type descriptor for the argument
    pub ty: Option<EcoString>,
}

impl FromStr for DocCommentSig {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim_end().ends_with('?') && !s.trim_end().ends_with(" ?")
            || !(s.chars()).all(|c| c.is_whitespace() || "?$:".contains(c) || is_ident_char(c))
        {
            return Err(());
        }
        // Split into args and outputs
        let mut label = false;
        let (mut outputs_text, mut args_text) = s
            .split_once('?')
            .or_else(|| s.split_once('$').inspect(|_| label = true))
            .ok_or(())?;
        outputs_text = outputs_text.trim();
        args_text = args_text.trim();
        // Parse args and outputs
        let mut args = Vec::new();
        let mut outputs = Vec::new();
        for (args, text) in [(&mut args, args_text), (&mut outputs, outputs_text)] {
            // Tokenize text
            let mut tokens = Vec::new();
            for frag in text.split_whitespace() {
                for (i, token) in frag.split(':').enumerate() {
                    if i > 0 {
                        tokens.push(":");
                    }
                    tokens.push(token);
                }
            }
            // Parse tokens into args
            let mut curr_arg_name = None;
            let mut tokens = tokens.into_iter().peekable();
            while let Some(token) = tokens.next() {
                if token == ":" {
                    let ty = tokens.next().unwrap_or_default();
                    args.push(DocCommentArg {
                        name: curr_arg_name.take().unwrap_or_default(),
                        ty: if ty.is_empty() { None } else { Some(ty.into()) },
                    });
                } else {
                    if let Some(curr) = curr_arg_name.take() {
                        args.push(DocCommentArg {
                            name: curr,
                            ty: None,
                        });
                    }
                    curr_arg_name = Some(token.into());
                }
            }
            if let Some(curr) = curr_arg_name.take() {
                args.push(DocCommentArg {
                    name: curr,
                    ty: None,
                });
            }
        }
        Ok(DocCommentSig {
            label,
            args: (!args.is_empty()).then_some(args),
            outputs: (!outputs.is_empty()).then_some(outputs),
        })
    }
}

impl From<String> for DocComment {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}

impl From<&str> for DocComment {
    fn from(text: &str) -> Self {
        let mut sig = None;
        let sig_line = text.lines().position(|line| {
            line.chars().filter(|&c| "$?".contains(c)).count() == 1
                && !line.trim().ends_with('?')
                && (line.chars())
                    .all(|c| c.is_whitespace() || "?$:".contains(c) || is_ident_char(c))
        });
        let raw_text = if let Some(i) = sig_line {
            sig = text.lines().nth(i).unwrap().parse().ok();

            let mut text: EcoString = (text.lines().take(i))
                .chain(["\n"])
                .chain(text.lines().skip(i + 1))
                .flat_map(|s| s.chars().chain(Some('\n')))
                .collect();
            while text.ends_with('\n') {
                text.pop();
            }
            if text.starts_with('\n') {
                text = text.trim_start_matches('\n').into();
            }
            text
        } else {
            text.into()
        };
        let mut text = EcoString::new();
        for (i, line) in raw_text.lines().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            text.push_str(line.trim());
        }
        DocComment { text, sig }
    }
}

/// A repository of code strings input to the compiler
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// A map of file paths to their string contents
    pub files: HashMap<PathBuf, EcoString>,
    /// A list of input strings without paths
    pub strings: EcoVec<EcoString>,
    /// A map of spans to macro strings
    pub macros: HashMap<CodeSpan, EcoString>,
}

impl Inputs {
    pub(crate) fn add_src(
        &mut self,
        src: impl IntoInputSrc,
        input: impl Into<EcoString>,
    ) -> InputSrc {
        let src = src.into_input_src(self.strings.len());
        match &src {
            InputSrc::File(path) => {
                self.files.insert(path.to_path_buf(), input.into());
            }
            InputSrc::Str(i) => {
                while self.strings.len() <= *i {
                    self.strings.push(EcoString::default());
                }
                self.strings.make_mut()[*i] = input.into();
            }
            InputSrc::Macro(span) => {
                self.macros.insert((**span).clone(), input.into());
            }
            InputSrc::Literal(_) => {}
        }
        src
    }
    /// Get an input string
    pub fn get(&self, src: &InputSrc) -> EcoString {
        match src {
            InputSrc::File(path) => self
                .files
                .get(&**path)
                .unwrap_or_else(|| panic!("File {:?} not found", path))
                .clone(),
            InputSrc::Str(index) => self
                .strings
                .get(*index)
                .unwrap_or_else(|| panic!("String {} not found", index))
                .clone(),
            InputSrc::Macro(span) => self
                .macros
                .get(span)
                .unwrap_or_else(|| panic!("Macro at {} not found", span))
                .clone(),
            InputSrc::Literal(s) => s.clone(),
        }
    }
    /// Get an input string and perform an operation on it
    #[track_caller]
    pub fn get_with<T>(&self, src: &InputSrc, f: impl FnOnce(&str) -> T) -> T {
        match src {
            InputSrc::File(path) => {
                if let Some(src) = self.files.get(&**path) {
                    f(&src)
                } else {
                    panic!(
                        "File {} not found. Available sources are {}",
                        path.display(),
                        self.available_srcs()
                    )
                }
            }
            InputSrc::Str(index) => {
                if let Some(src) = self.strings.get(*index) {
                    f(src)
                } else {
                    panic!(
                        "String {} not found. Available sources are {}",
                        index,
                        self.available_srcs()
                    )
                }
            }
            InputSrc::Macro(span) => {
                if let Some(src) = self.macros.get(span) {
                    f(src)
                } else {
                    panic!(
                        "Macro at {} not found. Available sources are {}",
                        span,
                        self.available_srcs()
                    )
                }
            }
            InputSrc::Literal(s) => f(s),
        }
    }
    fn available_srcs(&self) -> String {
        (self.files.iter().map(|e| e.0.display().to_string()))
            .chain(self.strings.iter().map(|i| format!("string {i}")))
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// Get an input string and perform an operation on it
    pub fn try_get_with<T>(&self, src: &InputSrc, f: impl FnOnce(&str) -> T) -> Option<T> {
        match src {
            InputSrc::File(path) => self.files.get(&**path).map(|src| f(src)),
            InputSrc::Str(index) => self.strings.get(*index).map(|src| f(src)),
            InputSrc::Macro(span) => self.macros.get(span).map(|src| f(&src)),
            InputSrc::Literal(s) => Some(f(s)),
        }
    }
}

impl fmt::Debug for Assembly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct FmtFunctions<'a>(&'a Assembly);
        impl fmt::Debug for FmtFunctions<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list()
                    .entries(self.0.bindings.iter().filter_map(|b| {
                        if let BindingKind::Func(func) = &b.kind {
                            Some((func, &self.0[func]))
                        } else {
                            None
                        }
                    }))
                    .finish()
            }
        }
        f.debug_struct("Assembly")
            .field("root", &self.root)
            .field("functions", &FmtFunctions(self))
            .finish()
    }
}
