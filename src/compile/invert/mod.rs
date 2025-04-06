mod un;
mod under;

use std::{
    boxed,
    cell::RefCell,
    collections::HashMap,
    error::Error,
    fmt,
    hash::{DefaultHasher, Hasher},
};

use ecow::eco_vec;

use crate::{
    assembly::{Assembly, Function},
    check::{nodes_clean_sig, nodes_sig, SigCheckError},
    ArrayLen, CustomInverse, FunctionId,
    ImplPrimitive::{self, *},
    Node::{self, *},
    Primitive::{self, *},
    abort_txt,
    Purity, SigNode, Signature, SysOp, Uiua, UiuaResult,
};

use un::*;

pub(crate) const DEBUG: bool = false;

macro_rules! dbgln {
    ($($arg:tt)*) => {
        if DEBUG {
            println!($($arg)*); // Allow println
        }
    }
}
use dbgln;

trait AsNode: fmt::Debug + Sync {
    fn as_node(&self, span: usize) -> Node;
}

impl AsNode for Node {
    fn as_node(&self, _: usize) -> Node {
        self.clone()
    }
}

impl AsNode for Primitive {
    fn as_node(&self, span: usize) -> Node {
        Node::Prim(*self, span)
    }
}

impl AsNode for ImplPrimitive {
    fn as_node(&self, span: usize) -> Node {
        Node::ImplPrim(*self, span)
    }
}

impl AsNode for i32 {
    fn as_node(&self, _: usize) -> Node {
        Node::new_push(*self)
    }
}

macro_rules! as_node {
    ($($T:ident),*) => {
        impl<$($T),*> AsNode for ($($T),*)
        where
            $($T: AsNode),*
        {
            #[allow(non_snake_case)]
            fn as_node(&self, span: usize) -> Node {
                let ($($T),*) = self;
                Node::from_iter([$($T.as_node(span)),*])
            }
        }
    };
}
as_node!(A, B);
as_node!(A, B, C);
as_node!(A, B, C, D);
as_node!(A, B, C, D, E);
as_node!(A, B, C, D, E, F);
as_node!(A, B, C, D, E, F, G, H, I);

trait SpanFromNodes: Sized + fmt::Debug + Sync {
    fn span_from_nodes<'a>(
        &self,
        nodes: &'a [Node],
        asm: &Assembly,
    ) -> Option<(&'a [Node], Option<usize>)>;
}

impl SpanFromNodes for Primitive {
    fn span_from_nodes<'a>(
        &self,
        nodes: &'a [Node],
        _: &Assembly,
    ) -> Option<(&'a [Node], Option<usize>)> {
        match nodes {
            [Node::Prim(prim, span), rest @ ..] if self == prim => Some((rest, Some(*span))),
            _ => None,
        }
    }
}

impl SpanFromNodes for ImplPrimitive {
    fn span_from_nodes<'a>(
        &self,
        nodes: &'a [Node],
        _: &Assembly,
    ) -> Option<(&'a [Node], Option<usize>)> {
        match nodes {
            [Node::ImplPrim(prim, span), rest @ ..] if self == prim => Some((rest, Some(*span))),
            _ => None,
        }
    }
}

impl SpanFromNodes for i32 {
    fn span_from_nodes<'a>(
        &self,
        nodes: &'a [Node],
        _: &Assembly,
    ) -> Option<(&'a [Node], Option<usize>)> {
        match nodes {
            [Node::Push(n), rest @ ..] if *n == *self => Some((rest, None)),
            _ => None,
        }
    }
}

macro_rules! span_from_nodes {
    ($($T:ident),*) => {
        impl<$($T),*> SpanFromNodes for ($($T),*)
        where
            $($T: SpanFromNodes),*
        {
            #[allow(non_snake_case)]
            fn span_from_nodes<'a>(&self, mut nodes: &'a [Node], asm: &Assembly) -> Option<(&'a [Node], Option<usize>)> {
                let ($($T),*) = self;
                let mut span = None;
                $(
                    let (new, sp) = $T.span_from_nodes(nodes, asm)?;
                    span = span.or(sp);
                    nodes = new;
                )*
                Some((nodes, span))
            }
        }
    };
}
span_from_nodes!(A, B);
span_from_nodes!(A, B, C, D);
span_from_nodes!(A, B, C, D, E);

/// Optionally allow a leading value
///
/// The value will not be pushed during the "undo" step
#[derive(Debug)]
struct MaybeVal<P>(P);

/// Require a leading value
///
/// The value will not be pushed during the "undo" step
#[derive(Debug)]
struct RequireVal<P>(P);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum InversionError {
    #[default]
    Generic,
    TooManyNodeuctions,
    Signature(SigCheckError),
    InnerFunc(Vec<FunctionId>, boxed::Box<Self>),
    AsymmetricUnderSig(Signature),
    ComplexInvertedUnder,
    UnderExperimental,
    AlgebraError(AlgebraError),
    UnUnderExperimental,
    UnUnderSignature(Signature),
    ReduceFormat,
}

pub type InversionResult<T = ()> = Result<T, InversionError>;

impl fmt::Display for InversionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InversionError::Generic => write!(f, "No inverse found"),
            InversionError::TooManyNodeuctions => {
                write!(f, "Function has too many instructions to invert")
            }
            InversionError::Signature(e) => write!(f, "Cannot invert invalid signature: {e}"),
            InversionError::InnerFunc(ids, inner) => {
                write!(f, "Inversion failed:")?;
                for id in ids {
                    write!(f, " cannot invert {id} because")?;
                }
                let inner = inner.to_string().to_lowercase();
                write!(f, " {inner}")
            }
            InversionError::AsymmetricUnderSig(sig) => {
                write!(
                    f,
                    "Cannot invert under with asymmetric \
                    second function signature {sig}"
                )
            }
            InversionError::ComplexInvertedUnder => {
                write!(f, "This under itself is too complex to invert")
            }
            InversionError::UnderExperimental => {
                write!(
                    f,
                    "Inversion of {} is experimental. To enable it, \
                    add `# Experimental!` to the top of the file.",
                    Primitive::Under.format()
                )
            }
            InversionError::AlgebraError(e) => e.fmt(f),
            InversionError::UnUnderExperimental => {
                write!(
                    f,
                    "{} {} is experimental. To enable it, \
                    add `# Experimental!` to the top of the file.",
                    Primitive::Un.format(),
                    Primitive::Under.format()
                )
            }
            InversionError::UnUnderSignature(sig) => {
                write!(
                    f,
                    "{} {}'s first function must have a net-zero \
                    signature, but its signature is {}",
                    Primitive::Un.format(),
                    Primitive::Under.format(),
                    sig
                )
            }
            InversionError::ReduceFormat => write!(
                f,
                "Only format functions with 2 arguments \
                and text only between them can be inverted"
            ),
        }
    }
}

impl InversionError {
    fn func(self, f: &Function) -> Self {
        match self {
            InversionError::InnerFunc(mut ids, inner) => {
                ids.push(f.id.clone());
                InversionError::InnerFunc(ids, inner)
            }
            e => InversionError::InnerFunc(vec![f.id.clone()], e.into()),
        }
    }
}

impl From<SigCheckError> for InversionError {
    fn from(e: SigCheckError) -> Self {
        InversionError::Signature(e)
    }
}
impl From<()> for InversionError {
    fn from(_: ()) -> Self {
        InversionError::Generic
    }
}

impl Error for InversionError {}

use ecow::{EcoString, EcoVec};
use InversionError::Generic;

/// A generic inversion error
fn generic<T>() -> InversionResult<T> {
    Err(InversionError::Generic)
}

pub(crate) fn match_format_pattern(parts: EcoVec<EcoString>, env: &mut Uiua) -> UiuaResult {
    abort_txt("you really though that tinyuiua has fmt pattern matching?");
}
