//! Uiua's Language Server Protocol (LSP) implementation
//!
//! Even without the `lsp` feature enabled, this module still provides some useful types and functions for working with Uiua code in an IDE or text editor.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    path::PathBuf,
    slice,
};

use crate::{
    ast::{Func, InlineMacro, Item, Modifier, ModuleKind, Ref, RefComponent, Word},
    ident_modifier_args, is_custom_glyph,
    lex::{CodeSpan, Sp},
    parse::parse,
    Assembly, BindingInfo, BindingKind, BindingMeta, Compiler, Ident, InputSrc, Inputs, LocalName,
    PreEvalMode, Primitive, Purity, SafeSys, Shape, Signature, SysBackend, UiuaError, Value,
    CONSTANTS,
};

/// Kinds of span in Uiua code, meant to be used in the language server or other IDE tools
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanKind {
    Primitive(Primitive, Option<i32>),
    String,
    Number,
    Comment,
    OutputComment,
    Strand,
    Ident {
        /// The documentation of the identifier
        docs: Option<BindingDocs>,
        /// Whether the identifier is the original binding name
        original: bool,
    },
    Label,
    Signature,
    Whitespace,
    Placeholder(usize),
    Delimiter,
    FuncDelim(Signature, SetInverses),
    MacroDelim(usize),
    ImportSrc(ImportSrc),
    Subscript(Option<Primitive>, Option<i32>),
    Obverse(SetInverses),
}

/// Documentation information for a binding
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingDocs {
    /// The span of the binding name where it was defined
    pub src_span: CodeSpan,
    /// Whether the binding is public
    pub is_public: bool,
    /// The specific binding kind
    pub kind: BindingDocsKind,
    /// An escape code used to type a glyph
    pub escape: Option<String>,
    /// Metadata about the binding
    pub meta: BindingMeta,
}

/// The kind of a binding
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingDocsKind {
    /// A constant
    Constant(Option<Value>),
    /// A function
    Function {
        /// The signature of the function
        sig: Signature,
        /// Whether the function is invertible
        invertible: bool,
        /// Whether the function is underable
        underable: bool,
        /// Whether the function is pure
        pure: bool,
    },
    /// A modifier
    Modifier(usize),
    /// A module
    Module {
        /// The signature of the module's `New` function
        sig: Option<Signature>,
    },
    /// An error
    Error,
}

/// Span data extracted from Uiua code
#[derive(Debug)]
pub struct Spans {
    /// The spans
    pub spans: Vec<Sp<SpanKind>>,
    /// The inputs used to build the spans
    pub inputs: Inputs,
    /// Top-level values for lines
    pub top_level_values: BTreeMap<usize, Vec<Value>>,
}

impl Spans {
    /// Get spans and their kinds from Uiua code
    pub fn from_input(input: &str) -> Self {
        Self::with_backend(input, SafeSys::default())
    }
    /// Get spans and their kinds from Uiua code with a custom backend
    pub fn with_backend(input: &str, backend: impl SysBackend) -> Self {
        let src = InputSrc::Str(0);
        let (items, _, _) = parse(input, src.clone(), &mut Inputs::default());
        let spanner = Spanner::new(src, input, backend);
        let spans = spanner.items_spans(&items);
        let inputs = spanner.asm.inputs;
        let top_level_values = spanner
            .code_meta
            .top_level_values
            .into_iter()
            .map(|(span, vals)| (span.start.line as usize, vals))
            .collect();
        Spans {
            spans,
            inputs,
            top_level_values,
        }
    }
    #[doc(hidden)]
    /// Get spans using the given compiler
    pub fn with_compiler(input: &str, compiler: &Compiler) -> Self {
        let mut compiler = compiler.clone();
        let src = InputSrc::Str(compiler.asm.inputs.strings.len().saturating_sub(1));
        let (items, _, _) = parse(input, src.clone(), &mut compiler.asm.inputs);
        let spanner = Spanner {
            src,
            asm: compiler.asm,
            code_meta: compiler.code_meta,
            errors: Vec::new(),
            diagnostics: Vec::new(),
        };
        let spans = spanner.items_spans(&items);
        let inputs = spanner.asm.inputs;
        let top_level_values = spanner
            .code_meta
            .top_level_values
            .into_iter()
            .map(|(span, vals)| (span.start.line as usize, vals))
            .collect();
        Spans {
            spans,
            inputs,
            top_level_values,
        }
    }
}

/// Code metadata for use in IDE tools
#[derive(Debug, Clone, Default)]
pub struct CodeMeta {
    /// A map of references to global bindings
    pub global_references: HashMap<CodeSpan, usize>,
    /// A map of references to shadowable constants
    pub constant_references: HashSet<Sp<Ident>>,
    /// Spans of functions and their signatures and whether they are explicit
    pub function_sigs: SigDecls,
    /// A map of macro invocations to their expansions
    pub macro_expansions: HashMap<CodeSpan, (Option<Ident>, String)>,
    /// A map of inline macro functions to their number of arguments
    pub inline_macros: HashMap<CodeSpan, usize>,
    /// A map of incomplete ref paths to their module's index
    pub incomplete_refs: HashMap<CodeSpan, usize>,
    /// A map of top-level binding names to their indices
    pub top_level_names: HashMap<Ident, LocalName>,
    /// A map of the spans of top-level lines to values
    pub top_level_values: HashMap<CodeSpan, Vec<Value>>,
    /// A map of strand spans
    pub strands: BTreeMap<CodeSpan, Vec<CodeSpan>>,
    /// A map of inner array spans
    pub array_inner_spans: BTreeMap<CodeSpan, Vec<CodeSpan>>,
    /// A map of array shapes
    pub array_shapes: BTreeMap<CodeSpan, Shape>,
    /// A map of module spans to their source
    pub import_srcs: HashMap<CodeSpan, ImportSrc>,
    /// A map of obverse spans to their set inverses
    pub obverses: HashMap<CodeSpan, SetInverses>,
}

/// Data for the signature of a function
#[derive(Debug, Clone, Copy)]
pub struct SigDecl {
    /// The signature itself
    pub sig: Signature,
    /// Whether the signature is explicitely declared
    pub explicit: bool,
    /// Whether the function is inline
    pub inline: bool,
    /// Inverses
    pub set_inverses: SetInverses,
}

/// Which inverses were set by `obverse`
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetInverses {
    pub un: bool,
    pub anti: bool,
    pub under: bool,
}

impl SetInverses {
    /// Whether no inverses were set
    pub fn is_empty(&self) -> bool {
        !(self.un || self.anti || self.under)
    }
}

struct FormatSetInverses<'a>(SetInverses, [&'a str; 3]);
impl fmt::Display for FormatSetInverses<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let FormatSetInverses(set_inverses, names) = self;
        let count =
            set_inverses.un as usize + set_inverses.anti as usize + set_inverses.under as usize;
        if count == 0 {
            return Ok(());
        }
        write!(f, "Sets ")?;
        for (i, (is_set, name)) in [set_inverses.un, set_inverses.anti, set_inverses.under]
            .into_iter()
            .zip(names)
            .enumerate()
        {
            if !is_set {
                continue;
            }
            if i > 0 {
                match count {
                    2 => write!(f, " and ")?,
                    3 if i == 1 => write!(f, ", ")?,
                    3 if i == 2 => write!(f, ", and ")?,
                    _ => {}
                }
            }
            write!(f, "{name}")?;
        }
        write!(f, " inverse{} here", if count == 1 { "" } else { "s" })
    }
}

impl fmt::Display for SetInverses {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        FormatSetInverses(*self, ["° un", "⌝ anti", "⍜ under"]).fmt(f)
    }
}

/// The source of an imported module
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSrc {
    /// A Git URL
    Git(String),
    /// A file path
    File(PathBuf),
}

pub(crate) type SigDecls = BTreeMap<CodeSpan, SigDecl>;

struct Spanner {
    src: InputSrc,
    asm: Assembly,
    code_meta: CodeMeta,
    #[allow(dead_code)]
    errors: Vec<UiuaError>,
    #[allow(dead_code)]
    diagnostics: Vec<crate::Diagnostic>,
}

impl Spanner {
    fn new(src: InputSrc, input: &str, backend: impl SysBackend) -> Self {
        let mut compiler = Compiler::with_backend(backend);
        compiler.pre_eval_mode(PreEvalMode::Lsp);
        compiler.backend().set_output_enabled(false);
        let errors = match compiler.load_str_src(input, src.clone()) {
            Ok(_) => Vec::new(),
            Err(e) => e.into_multi(),
        };
        let diagnostics = compiler.take_diagnostics().into_iter().collect();
        Self {
            src,
            asm: compiler.asm,
            code_meta: compiler.code_meta,
            errors,
            diagnostics,
        }
    }
    fn inputs(&self) -> &Inputs {
        &self.asm.inputs
    }
    fn items_spans(&self, items: &[Item]) -> Vec<Sp<SpanKind>> {
        let mut spans = Vec::new();
        for item in items {
            match item {
                Item::Module(m) => {
                    spans.push(m.value.open_span.clone().sp(SpanKind::Delimiter));
                    match &m.value.kind {
                        ModuleKind::Named(name) => {
                            let binding_docs = self.binding_docs(&name.span);
                            spans.push(name.span.clone().sp(SpanKind::Ident {
                                docs: binding_docs,
                                original: true,
                            }));
                        }
                        ModuleKind::Test => {}
                    }
                    if let Some(line) = &m.value.imports {
                        spans.push(line.tilde_span.clone().sp(SpanKind::Delimiter));
                        for item in &line.items {
                            let binding_docs = self.reference_docs(&item.span);
                            spans.push(item.span.clone().sp(SpanKind::Ident {
                                docs: binding_docs,
                                original: false,
                            }));
                        }
                    }
                    spans.extend(self.items_spans(&m.value.items));
                    if let Some(close_span) = &m.value.close_span {
                        spans.push(close_span.clone().sp(SpanKind::Delimiter));
                    }
                }
                Item::Words(lines) => {
                    for line in lines {
                        spans.extend(self.words_spans(line))
                    }
                }
                Item::Binding(binding) => {
                    if let Some(tilde_span) = &binding.tilde_span {
                        spans.push(tilde_span.clone().sp(SpanKind::Delimiter));
                    }
                    let binding_docs = self
                        .binding_docs(&binding.name.span)
                        .or_else(|| self.reference_docs(&binding.name.span));
                    spans.push(binding.name.span.clone().sp(SpanKind::Ident {
                        docs: binding_docs,
                        original: true,
                    }));
                    spans.push(binding.arrow_span.clone().sp(SpanKind::Delimiter));
                    if let Some(sig) = &binding.signature {
                        spans.push(sig.span.clone().sp(SpanKind::Signature));
                    }
                    spans.extend(self.words_spans(&binding.words));
                }
                Item::Data(data) => {
                    spans.push(data.init_span.clone().sp(SpanKind::Delimiter));
                    if let Some(name) = &data.name {
                        spans.push(name.span.clone().sp(SpanKind::Ident {
                            docs: self.binding_docs(&name.span),
                            original: true,
                        }));
                        if let Some(fields) = &data.fields {
                            spans.push(
                                name.span
                                    .clone()
                                    .end_to(&fields.open_span)
                                    .sp(SpanKind::Whitespace),
                            );
                        }
                    }
                    if let Some(fields) = &data.fields {
                        spans.push(fields.open_span.clone().sp(SpanKind::Delimiter));
                        let mut prev: Option<CodeSpan> = None;
                        for field in &fields.fields {
                            if let Some(prev) = prev {
                                let curr = field.span();
                                if prev.end.line == curr.start.line {
                                    spans.push(prev.end_to(&curr).sp(SpanKind::Whitespace))
                                }
                            }
                            spans.push(field.name.span.clone().sp(SpanKind::Ident {
                                docs: self.binding_docs(&field.name.span),
                                original: true,
                            }));
                            if let Some(validator) = &field.validator {
                                spans.push(validator.open_span.clone().sp(SpanKind::Delimiter));
                                spans.extend(self.words_spans(&validator.words));
                                if let Some(close_span) = &validator.close_span {
                                    spans.push(close_span.clone().sp(SpanKind::Delimiter));
                                }
                            }
                            if let Some(init) = &field.init {
                                spans.push(init.arrow_span.clone().sp(SpanKind::Delimiter));
                                spans.extend(self.words_spans(&init.words));
                            }
                            prev = Some(field.span());
                        }
                        if let Some(span) = &fields.close_span {
                            spans.push(span.clone().sp(SpanKind::Delimiter));
                        }
                    }
                    if let Some(words) = &data.func {
                        spans.extend(self.words_spans(words));
                    }
                }
                Item::Import(import) => {
                    if let Some(name) = &import.name {
                        let binding_docs = self.binding_docs(&name.span);
                        spans.push(name.span.clone().sp(SpanKind::Ident {
                            docs: binding_docs,
                            original: false,
                        }));
                    }
                    spans.push(import.tilde_span.clone().sp(SpanKind::Delimiter));
                    spans.push(
                        if let Some(src) = self.code_meta.import_srcs.get(&import.path.span) {
                            (import.path.span.clone()).sp(SpanKind::ImportSrc(src.clone()))
                        } else {
                            import.path.span.clone().sp(SpanKind::String)
                        },
                    );
                    for line in import.lines.iter().flatten() {
                        spans.push(line.tilde_span.clone().sp(SpanKind::Delimiter));
                        for item in &line.items {
                            let binding_docs = self.reference_docs(&item.span);
                            spans.push(item.span.clone().sp(SpanKind::Ident {
                                docs: binding_docs,
                                original: false,
                            }));
                        }
                    }
                }
            }
        }
        spans.sort_by_key(|sp| sp.span.start);
        spans
    }

    fn binding_docs(&self, span: &CodeSpan) -> Option<BindingDocs> {
        for binding in &self.asm.bindings {
            if binding.span != *span {
                continue;
            }
            return Some(self.make_binding_docs(binding));
        }
        None
    }

    fn reference_docs(&self, span: &CodeSpan) -> Option<BindingDocs> {
        // Look in global references
        if let Some(binding) = self
            .code_meta
            .global_references
            .get(span)
            .and_then(|i| self.asm.bindings.get(*i))
        {
            return Some(self.make_binding_docs(binding));
        }
        // Look in constant references
        for name in &self.code_meta.constant_references {
            if name.span != *span {
                continue;
            }
            let Some(constant) = CONSTANTS.iter().find(|c| c.name == name.value) else {
                continue;
            };
            let path = if let InputSrc::File(path) = &self.src {
                Some(&**path)
            } else {
                None
            };
            let sys = &crate::SafeSys::new();
            let val = constant.value.resolve(path, sys);
            let meta = BindingMeta {
                ..Default::default()
            };
            return Some(BindingDocs {
                src_span: span.clone(),
                is_public: true,
                kind: BindingDocsKind::Constant(Some(val)),
                escape: None,
                meta,
            });
        }
        None
    }

    fn make_binding_docs(&self, binfo: &BindingInfo) -> BindingDocs {
        let mut meta = binfo.meta.clone();
        if meta.comment.is_none() {
            let name = binfo.span.as_str(&self.asm.inputs, |s| s.to_string());
            meta.comment = match name.as_str() {
                "🦈" | "🏳️‍⚧️" => Some("Trans rights".into()),
                "🤠" => Some("This town ain't big enough for the ∩ of us".into()),
                "👽" => Some("Ayy, lmao".into()),
                "🐈" | "😺" | "😸" | "😹" | "😻" | "😼" | "😽" | "🙀" | "🐱‍👤" => {
                    Some("Meow".into())
                }
                "🐕" | "🐶" | "🦮" | "🐕‍🦺" => Some("Woof".into()),
                "🐖" | "🐷" | "🐽" /* | "👮" */ => Some("Oink".into()),
                "🐄" | "🐮" => Some("Moo".into()),
                "🐸" => Some("Ribbit".into()),
                "ඞ" => Some("SUS".into()),
                _ => None,
            };
        }
        if meta.comment.is_none() {
            match &binfo.kind {
                BindingKind::Const(None) => meta.comment = Some("constant".into()),
                BindingKind::Import(_) | BindingKind::Module(_) | BindingKind::Scope(_) => {
                    meta.comment = Some("module".into())
                }
                BindingKind::IndexMacro(_) | BindingKind::CodeMacro(_) => {
                    meta.comment = Some("macro".into())
                }
                BindingKind::Func(_) => {}
                BindingKind::Const(_) => {}
                BindingKind::Error => {}
            }
        }
        let kind = match &binfo.kind {
            BindingKind::Const(val) => BindingDocsKind::Constant(val.clone()),
            BindingKind::Func(f) => BindingDocsKind::Function {
                sig: f.sig,
                invertible: self.asm[f].un_inverse(&self.asm).is_ok(),
                underable: self.asm[f]
                    .under_inverse(Signature::new(1, 1), false, &self.asm)
                    .is_ok(),
                pure: self.asm[f].is_pure(Purity::Pure, &self.asm),
            },
            BindingKind::IndexMacro(args) => BindingDocsKind::Modifier(*args),
            BindingKind::CodeMacro(_) => {
                BindingDocsKind::Modifier(binfo.span.as_str(self.inputs(), ident_modifier_args))
            }
            BindingKind::Import(_) | BindingKind::Scope(_) => BindingDocsKind::Module { sig: None },
            BindingKind::Module(m) => {
                let sig = if let Some(local) = m.names.get("Call").or_else(|| m.names.get("New")) {
                    self.asm.bindings[local.index].kind.sig()
                } else {
                    None
                };
                BindingDocsKind::Module { sig }
            }
            BindingKind::Error => BindingDocsKind::Error,
        };
        let escape = binfo.span.as_str(&self.asm.inputs, |s| {
            is_custom_glyph(s).then(|| {
                let c = s.chars().next().unwrap();
                format!("\\\\{:x}", c as u32)
            })
        });
        BindingDocs {
            src_span: binfo.span.clone(),
            is_public: binfo.public,
            kind,
            escape,
            meta,
        }
    }

    fn words_spans(&self, words: &[Sp<Word>]) -> Vec<Sp<SpanKind>> {
        let mut spans = Vec::new();
        'words: for word in words {
            match &word.value {
                Word::Number(_) => {
                    for prim in Primitive::all().filter(|p| p.is_constant()) {
                        if word.span.as_str(self.inputs(), |s| {
                            prim.name().starts_with(s) || prim.to_string() == *s
                        }) {
                            spans.push(word.span.clone().sp(SpanKind::Primitive(prim, None)));
                            continue 'words;
                        }
                    }
                    spans.push(word.span.clone().sp(SpanKind::Number))
                }
                Word::Char(_) | Word::String(_) | Word::FormatString(_) => {
                    spans.push(word.span.clone().sp(SpanKind::String))
                }
                Word::Label(_) => spans.push(word.span.clone().sp(SpanKind::Label)),
                Word::MultilineString(lines) => {
                    spans.extend((lines.iter()).map(|line| line.span.clone().sp(SpanKind::String)))
                }
                Word::MultilineFormatString(lines) => {
                    spans.extend((lines.iter()).map(|line| line.span.clone().sp(SpanKind::String)))
                }
                Word::Ref(r) => spans.extend(self.ref_spans(r)),
                Word::IncompleteRef { path, .. } => spans.extend(self.ref_path_spans(path)),
                Word::Strand(items) => {
                    for i in 0..items.len() {
                        let word = &items[i];
                        if i > 0 {
                            let prev = &items[i - 1];
                            spans.push(
                                CodeSpan {
                                    start: prev.span.end,
                                    end: word.span.start,
                                    src: word.span.src.clone(),
                                }
                                .sp(SpanKind::Strand),
                            );
                        }
                        spans.extend(self.words_spans(slice::from_ref(word)));
                    }
                }
                Word::Array(arr) => {
                    spans.push(word.span.just_start(self.inputs()).sp(SpanKind::Delimiter));
                    if let Some(sig) = &arr.signature {
                        spans.push(sig.span.clone().sp(SpanKind::Signature));
                    }
                    spans.extend(arr.lines.iter().flat_map(|w| self.words_spans(w)));
                    if arr.closed {
                        let end = word.span.just_end(self.inputs());
                        if end.as_str(self.inputs(), |s| s == "]")
                            || end.as_str(self.inputs(), |s| s == "}")
                        {
                            spans.push(end.sp(SpanKind::Delimiter));
                        }
                    }
                }
                Word::Func(func) => spans.extend(self.func_spans(func, &word.span)),
                Word::Pack(pack) => {
                    let mut kind = if let Some(inline) = pack
                        .branches
                        .first()
                        .and_then(|br| self.code_meta.function_sigs.get(&br.span))
                    {
                        SpanKind::FuncDelim(inline.sig, inline.set_inverses)
                    } else {
                        SpanKind::Delimiter
                    };
                    spans.push(word.span.just_start(self.inputs()).sp(kind.clone()));
                    for (i, branch) in pack.branches.iter().enumerate() {
                        let start_span = branch.span.just_start(self.inputs());
                        if i > 0 && start_span.as_str(self.inputs(), |s| s == "|") {
                            kind = if let Some(SigDecl {
                                sig,
                                set_inverses,
                                explicit: false,
                                ..
                            }) = self.code_meta.function_sigs.get(&branch.span)
                            {
                                SpanKind::FuncDelim(*sig, *set_inverses)
                            } else {
                                SpanKind::Delimiter
                            };
                            spans.push(start_span.sp(kind.clone()));
                        }
                        if let Some(sig) = &branch.value.signature {
                            spans.push(sig.span.clone().sp(SpanKind::Signature));
                        }
                        spans.extend(branch.value.lines.iter().flat_map(|w| self.words_spans(w)));
                    }
                    if pack.closed {
                        let end = word.span.just_end(self.inputs());
                        if end.as_str(self.inputs(), |s| s == ")") {
                            spans.push(end.sp(kind));
                        }
                    }
                }
                Word::Primitive(prim) => {
                    spans.push(word.span.clone().sp(SpanKind::Primitive(*prim, None)))
                }
                Word::Modified(m) => {
                    match &m.modifier.value {
                        Modifier::Primitive(Primitive::Obverse) => {
                            spans.push(m.modifier.span.clone().sp(
                                if let Some(set_inverses) =
                                    self.code_meta.obverses.get(&m.modifier.span)
                                {
                                    SpanKind::Obverse(*set_inverses)
                                } else {
                                    SpanKind::Primitive(Primitive::Obverse, None)
                                },
                            ))
                        }
                        Modifier::Primitive(p) => {
                            spans.push((m.modifier.span.clone()).sp(SpanKind::Primitive(*p, None)))
                        }
                        Modifier::Ref(r) => spans.extend(self.ref_spans(r)),
                        Modifier::Macro(mac) => {
                            spans.extend(self.func_spans(&mac.func.value, &mac.func.span));
                            let mac_delim_kind =
                                SpanKind::MacroDelim(ident_modifier_args(&mac.ident.value));
                            if let Some(span) = &mac.caret_span {
                                spans.push(span.clone().sp(mac_delim_kind.clone()));
                            }
                            let ident_span = (mac.ident.span.clone()).sp(mac_delim_kind);
                            spans.push(ident_span);
                        }
                    }
                    spans.extend(self.words_spans(&m.operands));
                }
                Word::Spaces | Word::BreakLine | Word::FlipLine => {
                    spans.push(word.span.clone().sp(SpanKind::Whitespace))
                }
                Word::Comment(_) | Word::SemanticComment(_) => {
                    spans.push(word.span.clone().sp(SpanKind::Comment))
                }
                Word::OutputComment { .. } => {
                    spans.push(word.span.clone().sp(SpanKind::OutputComment))
                }
                Word::Placeholder(op) => {
                    spans.push(word.span.clone().sp(SpanKind::Placeholder(*op)))
                }
                #[allow(clippy::match_single_binding)]
                Word::Subscripted(sub) => {
                    let n = sub.n.value.n();
                    match &sub.word.value {
                        Word::Modified(m) => {
                            match &m.modifier.value {
                                Modifier::Primitive(p) => {
                                    spans.push(
                                        m.modifier.span.clone().sp(SpanKind::Primitive(*p, n)),
                                    );
                                    spans.push(
                                        sub.n.span.clone().sp(SpanKind::Subscript(Some(*p), n)),
                                    );
                                }
                                Modifier::Ref(r) => {
                                    spans.extend(self.ref_spans(r));
                                    spans.push(sub.n.span.clone().sp(SpanKind::Subscript(None, n)));
                                }
                                Modifier::Macro(mac) => {
                                    spans.extend(self.func_spans(&mac.func.value, &mac.func.span));
                                    let mac_delim_kind =
                                        SpanKind::MacroDelim(ident_modifier_args(&mac.ident.value));
                                    if let Some(span) = &mac.caret_span {
                                        spans.push(span.clone().sp(mac_delim_kind.clone()));
                                    }
                                    let ident_span = (mac.ident.span.clone()).sp(mac_delim_kind);
                                    spans.push(ident_span);
                                    spans.push(sub.n.span.clone().sp(SpanKind::Subscript(None, n)));
                                }
                            }
                            spans.extend(self.words_spans(&m.operands));
                        }
                        Word::Primitive(p) => {
                            spans.push((sub.word.span.clone()).sp(SpanKind::Primitive(*p, n)));
                            spans.push(sub.n.span.clone().sp(SpanKind::Subscript(Some(*p), n)));
                        }
                        _ => {
                            spans.extend(self.words_spans(slice::from_ref(&sub.word)));
                            spans.push(sub.n.span.clone().sp(SpanKind::Subscript(None, n)));
                        }
                    }
                }
                Word::InlineMacro(InlineMacro {
                    ident,
                    caret_span,
                    func,
                }) => {
                    spans.extend(self.func_spans(&func.value, &func.span));
                    let mac_delim_kind = SpanKind::MacroDelim(ident_modifier_args(&ident.value));
                    if let Some(span) = caret_span {
                        spans.push(span.clone().sp(mac_delim_kind.clone()));
                    }
                    spans.push(ident.span.clone().sp(mac_delim_kind));
                }
            }
        }
        spans.retain(|sp| !sp.span.as_str(self.inputs(), str::is_empty));
        spans
    }
    fn ref_spans(&self, r: &Ref) -> Vec<Sp<SpanKind>> {
        let mut spans = self.ref_path_spans(&r.path);
        let docs = self.reference_docs(&r.name.span);
        spans.push(r.name.span.clone().sp(SpanKind::Ident {
            docs,
            original: false,
        }));
        spans
    }
    fn ref_path_spans(&self, path: &[RefComponent]) -> Vec<Sp<SpanKind>> {
        let mut spans = Vec::new();
        for comp in path {
            let docs = self.reference_docs(&comp.module.span);
            spans.push(comp.module.span.clone().sp(SpanKind::Ident {
                docs,
                original: false,
            }));
            spans.push(comp.tilde_span.clone().sp(SpanKind::Delimiter));
        }
        spans
    }
    fn func_spans(&self, func: &Func, span: &CodeSpan) -> Vec<Sp<SpanKind>> {
        let mut spans = Vec::new();
        let kind = if let Some(inline) = self.code_meta.function_sigs.get(span) {
            SpanKind::FuncDelim(inline.sig, inline.set_inverses)
        } else if let Some(margs) = self.code_meta.inline_macros.get(span) {
            SpanKind::MacroDelim(*margs)
        } else {
            SpanKind::Delimiter
        };
        spans.push(span.just_start(self.inputs()).sp(kind.clone()));
        if let Some(sig) = &func.signature {
            spans.push(sig.span.clone().sp(SpanKind::Signature));
        }
        spans.extend(func.lines.iter().flat_map(|w| self.words_spans(w)));
        if func.closed {
            let end = span.just_end(self.inputs());
            if end.as_str(self.inputs(), |s| s == ")") || end.as_str(self.inputs(), |s| s == "}") {
                spans.push(end.sp(kind));
            }
        }
        spans
    }
}
