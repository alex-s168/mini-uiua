//! Primitive definitions and top-level implementations
//!
//! For the meat of the actual array algorithms, see [`crate::algorithm`].

mod defs;
pub use defs::*;

use core::str;
use std::{
    collections::HashMap,
    f64::consts::{PI, TAU},
    fmt
};

use enum_iterator::{all, Sequence};

use crate::{
    algorithm::{self, loops, reduce, table, zip, *},
    grid_fmt::GridFmt,
    lex::{AsciiToken, SUBSCRIPT_DIGITS},
    sys::*,
    value::*,
    randf, shuffle,
    FunctionId, Ops, Shape, Signature, Uiua, UiuaErrorKind, UiuaResult,
};

/// Categories of primitives
#[derive(Clone, Copy, PartialEq, Eq, Hash, Sequence)]
#[allow(missing_docs)]
pub enum PrimClass {
    Stack,
    Constant,
    MonadicPervasive,
    DyadicPervasive,
    MonadicArray,
    DyadicArray,
    IteratingModifier,
    AggregatingModifier,
    InversionModifier,
    Planet,
    OtherModifier,
    Comptime,
    Debug,
    Thread,
    Map,
    Encoding,
    Misc,
    Sys(SysOpClass),
}

impl PrimClass {
    /// Get an iterator over all primitive classes
    pub fn all() -> impl Iterator<Item = Self> {
        all()
    }
    /// Check if this class is pervasive
    pub fn is_pervasive(&self) -> bool {
        matches!(
            self,
            PrimClass::MonadicPervasive | PrimClass::DyadicPervasive
        )
    }
    /// Get an iterator over all primitives in this class
    pub fn primitives(self) -> impl Iterator<Item = Primitive> {
        Primitive::all().filter(move |prim| prim.class() == self)
    }
}

impl fmt::Debug for PrimClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PrimClass::*;
        match self {
            Stack => write!(f, "Stack"),
            Constant => write!(f, "Constant"),
            MonadicPervasive => write!(f, "MonadicPervasive"),
            DyadicPervasive => write!(f, "DyadicPervasive"),
            MonadicArray => write!(f, "MonadicArray"),
            DyadicArray => write!(f, "DyadicArray"),
            IteratingModifier => write!(f, "IteratingModifier"),
            AggregatingModifier => write!(f, "AggregatingModifier"),
            InversionModifier => write!(f, "InversionModifier"),
            Planet => write!(f, "Planet"),
            OtherModifier => write!(f, "OtherModifier"),
            Comptime => write!(f, "Comptime"),
            Debug => write!(f, "Debug"),
            Thread => write!(f, "Thread"),
            Map => write!(f, "Map"),
            Encoding => write!(f, "Encoding"),
            Misc => write!(f, "Misc"),
            Sys(op) => op.fmt(f),
        }
    }
}

/// The names of a primitive
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrimNames {
    /// The text name
    pub text: &'static str,
    /// An ASCII token that formats to the primitive
    pub ascii: Option<AsciiToken>,
    /// The primitive's glyph
    pub glyph: Option<char>,
}

impl From<&'static str> for PrimNames {
    fn from(text: &'static str) -> Self {
        Self {
            text,
            ascii: None,
            glyph: None,
        }
    }
}
impl From<(&'static str, char)> for PrimNames {
    fn from((text, glyph): (&'static str, char)) -> Self {
        Self {
            text,
            ascii: None,
            glyph: Some(glyph),
        }
    }
}
impl From<(&'static str, AsciiToken, char)> for PrimNames {
    fn from((text, ascii, glyph): (&'static str, AsciiToken, char)) -> Self {
        Self {
            text,
            ascii: Some(ascii),
            glyph: Some(glyph),
        }
    }
}

impl fmt::Display for Primitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(c) = self.glyph() {
            write!(f, "{}", c)
        } else if let Some(s) = self.ascii() {
            write!(f, "{}", s)
        } else {
            write!(f, "{}", self.name())
        }
    }
}

fn fmt_subscript(f: &mut fmt::Formatter<'_>, mut i: i32) -> fmt::Result {
    if i < 0 {
        write!(f, "₋")?;
        i = -i;
    }
    while i > 0 {
        write!(f, "{}", SUBSCRIPT_DIGITS[i as usize % 10])?;
        i /= 10;
    }
    Ok(())
}

impl fmt::Display for ImplPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ImplPrimitive::*;
        use Primitive::*;
        match self {
            &DeshapeSub(i) => {
                write!(f, "{Deshape}")?;
                fmt_subscript(f, i)
            }
            &EachSub(i) => {
                write!(f, "{Each}")?;
                fmt_subscript(f, i)
            }
            OnSub(i) => {
                write!(f, "{On}")?;
                fmt_subscript(f, *i as i32)
            }
            BySub(i) => {
                write!(f, "{By}")?;
                fmt_subscript(f, *i as i32)
            }
            WithSub(i) => {
                write!(f, "{With}")?;
                fmt_subscript(f, *i as i32)
            }
            OffSub(i) => {
                write!(f, "{Off}")?;
                fmt_subscript(f, *i as i32)
            }
            NBits(n) => {
                write!(f, "{Bits}")?;
                fmt_subscript(f, *n as i32)
            }
            Root => write!(f, "{Anti}{Pow}"),
            Cos => write!(f, "cos"),
            Asin => write!(f, "{Un}{Sin}"),
            Acos => write!(f, "{Un}{Cos}"),
            UnPop => write!(f, "{Un}{Pop}"),
            UnBits => write!(f, "{Un}{Bits}"),
            UnWhere => write!(f, "{Un}{Where}"),
            UnCouple => write!(f, "{Un}{Couple}"),
            UnMap => write!(f, "{Un}{Map}"),
            UnAtan => write!(f, "{Un}{Atan}"),
            UnUtf8 => write!(f, "{Un}{Utf8}"),
            UnUtf16 => write!(f, "{Un}{Utf16}"),
            Utf16 => write!(f, "utf₁₆"),
            UnGraphemes => write!(f, "{Un}{Graphemes}"),
            UnParse => write!(f, "{Un}{Parse}"),
            UnFix => write!(f, "{Un}{Fix}"),
            UnShape => write!(f, "{Un}{Shape}"),
            AntiDrop => write!(f, "{Anti}{Drop}"),
            AntiSelect => write!(f, "{Anti}{Select}"),
            AntiPick => write!(f, "{Anti}{Pick}"),
            UnJoin | UnJoinShape | UnJoinShape2 => write!(f, "{Un}{Join}"),
            UnJoinEnd | UnJoinShapeEnd | UnJoinShape2End => write!(f, "{Un}({Join}{Flip})"),
            UnKeep => write!(f, "{Un}{Keep}"),
            UnScan => write!(f, "{Un}{Scan}"),
            UnStack => write!(f, "{Un}{Stack}"),
            UnDump => write!(f, "{Un}{Dump}"),
            UnFill => write!(f, "{Un}{Fill}"),
            UnBox => write!(f, "{Un}{Box}"),
            UnSort => write!(f, "{Un}{Sort}"),
            UnDatetime => write!(f, "{Un}{DateTime}"),
            UnBoth => write!(f, "{Un}{Both}"),
            UnBracket => write!(f, "{Un}{Bracket}"),
            ProgressiveIndexOf => write!(f, "{Un}{By}{Select}"),
            UndoUnBits => write!(f, "{Under}{Un}{Bits}"),
            AntiBase => write!(f, "{Anti}{Base}"),
            UndoReverse { n, .. } => write!(f, "{Under}{Reverse}({n})"),
            UndoTransposeN(n, _) => write!(f, "{Under}{Transpose}({n})"),
            UndoRotate(n) => write!(f, "{Under}{Rotate}({n})"),
            UndoTake => write!(f, "{Under}{Take}"),
            UndoDrop => write!(f, "{Under}{Drop}"),
            UndoSelect => write!(f, "{Under}{Select}"),
            UndoPick => write!(f, "{Under}{Pick}"),
            UndoWhere => write!(f, "{Under}{Where}"),
            AntiOrient => write!(f, "{Anti}{Orient}"),
            UndoAntiOrient => write!(f, "{Under}{Orient}"),
            UndoInsert => write!(f, "{Under}{Insert}"),
            UndoRemove => write!(f, "{Under}{Remove}"),
            UndoPartition1 | UndoPartition2 => write!(f, "{Under}{Partition}"),
            UndoGroup1 | UndoGroup2 => write!(f, "{Under}{Group}"),
            TryClose => write!(f, "{}", Sys(SysOp::Close)),
            UndoFix => write!(f, "{Under}{Fix}"),
            UndoDeshape(_) => write!(f, "{Under}{Deshape}"),
            UndoFirst => write!(f, "{Under}{First}"),
            UndoLast => write!(f, "{Under}{Last}"),
            UndoKeep => write!(f, "{Under}{Keep}"),
            UndoRerank => write!(f, "{Under}{Rerank}"),
            UndoReshape => write!(f, "{Un}{Reshape}"),
            UndoWindows => write!(f, "{Un}{Stencil}{Identity}"),
            UndoJoin => write!(f, "{Under}{Join}"),
            UndoRows => write!(f, "{Under}{Rows}"),
            UndoInventory => write!(f, "{Under}{Inventory}"),
            MaxRowCount(n) => write!(f, "MaxRowCount({n})"),
            SetSign => write!(f, "{Under}{Sign}"),
            // Optimizations
            FirstMinIndex => write!(f, "{First}{Rise}"),
            FirstMaxIndex => write!(f, "{First}{Fall}"),
            LastMinIndex => write!(f, "{First}{Reverse}{Rise}"),
            LastMaxIndex => write!(f, "{First}{Reverse}{Fall}"),
            FirstWhere => write!(f, "{First}{Where}"),
            LastWhere => write!(f, "{First}{Reverse}{Where}"),
            LenWhere => write!(f, "{Len}{Where}"),
            MemberOfRange => write!(f, "{MemberOf}{Range}"),
            MultidimMemberOfRange => write!(f, "{MemberOf}{Rerank}1{Range}"),
            RandomRow => write!(f, "{First}{Un}{Sort}"),
            SortDown => write!(f, "{Select}{Fall}{Dup}"),
            AllSame => write!(f, "all same"),
            Primes => write!(f, "{Un}{Reduce}{Mul}"),
            ReplaceRand => write!(f, "{Gap}{Rand}"),
            ReplaceRand2 => write!(f, "{Gap}{Gap}{Rand}"),
            ReduceContent => write!(f, "{Reduce}{Content}"),
            ReduceConjoinInventory => write!(f, "{Reduce}{Content}{Join}{Inventory}"),
            ReduceTable => write!(f, "{Reduce}(…){Table}"),
            CountUnique => write!(f, "{Len}{Deduplicate}"),
            MatchPattern => write!(f, "pattern match"),
            MatchLe => write!(f, "match ≤"),
            MatchGe => write!(f, "match ≥"),
            AstarFirst => write!(f, "{First}{Astar}"),
            AstarTake => write!(f, "{Take}…{Astar}"),
            AstarPop => write!(f, "{Pop}{Astar}"),
            PathFirst => write!(f, "{First}{Path}"),
            PathTake => write!(f, "{Take}…{Path}"),
            PathPop => write!(f, "{Pop}{Path}"),
            SplitByScalar => write!(f, "{Partition}{Box}{By}{Ne}"),
            SplitBy => write!(f, "{Partition}{Box}{Not}{By}{Mask}"),
            SplitByKeepEmpty => write!(f, "{Un}{Reduce}$\"_…_\""),
            MatrixDiv => write!(f, "{Anti}{Under}{Transpose}({Reduce}{Add}{Mul})"),
            &ReduceDepth(n) => {
                for _ in 0..n {
                    write!(f, "{Rows}")?;
                }
                write!(f, "{Reduce}(…)")?;
                Ok(())
            }
            &TransposeN(n) => {
                if n < 0 {
                    write!(f, "{Un}")?;
                    if n < -1 {
                        write!(f, "(")?;
                    }
                }
                for _ in 0..n.unsigned_abs() {
                    write!(f, "{Transpose}")?;
                }
                if n < -1 {
                    write!(f, ")")?;
                }
                Ok(())
            }
            &StackN { n, inverse } => {
                if inverse {
                    write!(f, "{Un}")?;
                }
                let n_str: String = (n.to_string().chars())
                    .map(|c| SUBSCRIPT_DIGITS[(c as u32 as u8 - b'0') as usize])
                    .collect();
                write!(f, "{Stack}{n_str}")
            }
            RepeatWithInverse => write!(f, "{Repeat}"),
            RepeatCountConvergence => write!(f, "{Un}{Repeat}"),
            ValidateType => write!(f, "{Un}…{Type}{Dup}"),
            ValidateTypeConsume => write!(f, "{Un}…{Type}"),
            TestAssert => write!(f, "{Assert}"),
            ValidateNonBoxedVariant => write!(f, "|…[…]"),
            ValidateVariant => write!(f, "|…°[…]"),
            TagVariant => write!(f, "<tag variant>"),
        }
    }
}

macro_rules! constant {
    ($name:ident, $value:expr) => {
        fn $name() -> Value {
            $value.into()
        }
    };
}

constant!(eta, PI / 2.0);
constant!(pi, PI);
constant!(tau, TAU);
constant!(inf, f64::INFINITY);

/// A wrapper that nicely prints a `Primitive`
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormatPrimitive(pub Primitive);

impl fmt::Debug for FormatPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for FormatPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.glyph().is_none() {
            self.0.fmt(f)
        } else {
            write!(f, "{} {}", self.0, self.0.name())
        }
    }
}

static ALIASES: [(Primitive, &[&str]); 24] = [
    (Primitive::Identity, &["id"] as &[_]),
    (Primitive::Gap, &["ga"]),
    (Primitive::Pop, &["po"]),
    (Primitive::Fix, &["fx"]),
    (Primitive::Box, &["bx"]),
    (Primitive::IndexOf, &["idx"]),
    (Primitive::Switch, &["sw"]),
    (Primitive::Stencil, &["st", "win"]),
    (Primitive::Floor, &["flr", "flor"]),
    (Primitive::Range, &["ran"]),
    (Primitive::Partition, &["par"]),
    (Primitive::Dup, &["dup"]),
    (Primitive::Deshape, &["flat"]),
    (Primitive::Ne, &["ne", "neq"]),
    (Primitive::Eq, &["eq"]),
    (Primitive::Lt, &["lt"]),
    (Primitive::Le, &["le", "leq"]),
    (Primitive::Gt, &["gt"]),
    (Primitive::Ge, &["ge", "geq"]),
    (Primitive::Utf8, &["utf", "utf__8"]),
    (Primitive::First, &["fst"]),
    (Primitive::Last, &["lst"]),
    (Primitive::Slf, &["slf"]),
    (Primitive::Select, &["sel"]),
];

macro_rules! fill {
    ($ops:expr, $env:expr, $with:ident, $without_but:ident) => {{
        let env = $env;
        let [fill, f] = get_ops($ops, env)?;
        let outputs = fill.sig.outputs;
        if outputs > 1 {
            return Err(env.error(format!(
                "{} function can have at most 1 output, but its signature is {}",
                Primitive::Fill.format(),
                fill.sig
            )));
        }
        if outputs == 0 {
            return env.$without_but(fill.sig.args, |env| env.exec(fill), |env| env.exec(f));
        }
        env.exec(fill)?;
        let fill_value = env.pop("fill value")?;
        env.$with(fill_value, |env| env.exec(f))?;
    }};
}

impl Primitive {
    /// Get an iterator over all primitives
    pub fn all() -> impl Iterator<Item = Self> + Clone {
        all()
    }
    /// Get an iterator over all non-deprecated primitives
    pub fn non_deprecated() -> impl Iterator<Item = Self> + Clone {
        Self::all().filter(|p| !p.is_deprecated())
    }
    /// Get the primitive's name
    ///
    /// This is the name that is used for formatting
    pub fn name(&self) -> &'static str {
        self.names().text
    }
    /// Get the ASCII token that formats to the primitive
    pub fn ascii(&self) -> Option<AsciiToken> {
        self.names().ascii
    }
    /// Get the primitive's glyph
    pub fn glyph(&self) -> Option<char> {
        self.names().glyph
    }
    /// Find a primitive by its text name
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all().find(|p| p.name() == name)
    }
    /// Find a primitive by its ASCII token
    pub fn from_ascii(s: AsciiToken) -> Option<Self> {
        Self::all().find(|p| p.ascii() == Some(s))
    }
    /// Find a primitive by its glyph
    pub fn from_glyph(c: char) -> Option<Self> {
        Self::all().find(|p| p.glyph() == Some(c))
    }
    /// Get the primitive's signature, if it is always well-defined
    pub fn sig(&self) -> Option<Signature> {
        let (args, outputs) = self.args().zip(self.outputs())?;
        Some(Signature { args, outputs })
    }
    /// Check if this primitive is a modifier
    pub fn is_modifier(&self) -> bool {
        self.modifier_args().is_some()
    }
    /// Check if this primitive is a constant
    pub fn is_constant(&self) -> bool {
        self.constant().is_some()
    }
    /// Get the a constant's value
    pub fn constant(&self) -> Option<f64> {
        use Primitive::*;
        match self {
            Eta => Some(PI / 2.0),
            Pi => Some(PI),
            Tau => Some(TAU),
            Infinity => Some(f64::INFINITY),
            _ => None,
        }
    }
    /// Get a pretty-printable wrapper for this primitive
    pub fn format(&self) -> FormatPrimitive {
        FormatPrimitive(*self)
    }
    /// The modified signature of the primitive given a subscript
    pub fn subscript_sig(&self, n: Option<i32>) -> Option<Signature> {
        use Primitive::*;
        Some(match (self, n) {
            (prim, Some(_)) if prim.class() == PrimClass::DyadicPervasive => Signature::new(1, 1),
            (
                Select | Pick | Take | Drop | Join | Rerank | Rotate | Orient | Windows | Base,
                Some(_),
            ) => Signature::new(1, 1),
            (First | Last, Some(n)) if n >= 0 => Signature::new(1, n as usize),
            (Couple | Box, Some(n)) if n >= 0 => Signature::new(n as usize, 1),
            (Couple, None) => Signature::new(2, 1),
            (Box, None) => Signature::new(1, 1),
            (Transpose | Sqrt | Round | Floor | Ceil | Rand | Utf8, _) => return self.sig(),
            (Stack, Some(n)) if n >= 0 => Signature::new(n as usize, n as usize),
            _ => return None,
        })
    }
    pub(crate) fn deprecation_suggestion(&self) -> Option<String> {
        use Primitive::*;
        Some(match self {
            Sig => "use (⋅⊢)^! instead".into(),
            Stringify => "use (◇repr⊢)^! instead".into(),
            Rerank => format!(
                "use subscripted {} or {Un}{By}({Len}{Shape}) instead",
                Deshape.format()
            ),
            Trace => format!("use subscripted {} instead", Stack.format()),
            Windows => format!("use {} {} instead", Stencil.format(), Identity.format()),
            Astar => format!("use {} instead", Path.format()),
            Over => format!("use {With} or {Below} instead"),
            Around => format!("use {On}{Flip}, {Off}{Identity}, or sided subscripts instead"),
            _ => return None,
        })
    }
    /// Check if this primitive is experimental
    #[allow(unused_parens)]
    pub fn is_experimental(&self) -> bool {
        use Primitive::*;
        matches!(
            self,
            (Reach | Slf | Above | Around)
                | (Or | Base)
                | Astar
                | (Stringify | Quote | Sig)
        )
    }
    /// Check if this primitive is deprecated
    pub fn is_deprecated(&self) -> bool {
        self.deprecation_suggestion().is_some()
    }
    /// Get the short aliases for this primitive
    pub fn aliases(&self) -> &'static [&'static str] {
        ALIASES.iter().find(|(k,_)| k == self).unwrap().1
    }
    /// Try to parse a primitive from a name prefix
    pub fn from_format_name(name: &str) -> Option<Self> {
        if name.chars().any(char::is_uppercase) {
            return None;
        }
        if name.len() < 2 {
            return None;
        }
        let reverse_aliases: HashMap<&'static str, Primitive> = ALIASES
            .iter()
            .flat_map(|(prim, aliases)| aliases.iter().map(|&s| (s, *prim)))
            .collect();
        if let Some(prim) = reverse_aliases.get(name) {
            return Some(*prim);
        }
        if let Some(prim) = Primitive::non_deprecated().find(|p| p.name() == name) {
            return Some(prim);
        }
        if let Some(prim) = Primitive::all().find(|p| p.glyph().is_none() && p.name() == name) {
            return Some(prim);
        }
        if let Some(prim) = SysOp::ALL.iter().find(|s| s.name() == name) {
            return Some(Primitive::Sys(*prim));
        }
        if name.len() < 3 {
            return None;
        }
        let mut matching = Primitive::non_deprecated()
            .filter(|p| p.glyph().is_some() && p.name().starts_with(name));
        let res = matching.next()?;
        let exact_match = res.name() == name;
        (exact_match || matching.next().is_none()).then_some(res)
    }
    /// The list of strings where each character maps to an entire primitive
    pub fn multi_aliases() -> &'static [(&'static str, &'static [(Primitive, &'static str)])] {
        use Primitive::*;
        &[
            ("kork", &[(Keep, "k"), (On, "o"), (Rows, "r"), (Keep, "k")]),
            ("rkok", &[(Rows, "r"), (Keep, "k"), (On, "o"), (Keep, "k")]),
            ("awm", &[(Assert, "a"), (With, "w"), (Match, "m")]),
            ("dor", &[(Div, "d"), (On, "o"), (Range, "r")]),
            (
                "pbbn",
                &[(Partition, "p"), (Box, "b"), (By, "b"), (Ne, "n")],
            ),
            (
                "ppbn",
                &[(Partition, "p"), (Parse, "p"), (By, "b"), (Ne, "n")],
            ),
            (
                "pibn",
                &[(Partition, "p"), (Identity, "i"), (By, "b"), (Ne, "n")],
            ),
            ("kbn", &[(Keep, "k"), (By, "b"), (Ne, "n")]),
        ]
    }
    /// Look up a multi-alias from [`Self::multi_aliases`]
    pub fn get_multi_alias(name: &str) -> Option<&'static [(Primitive, &'static str)]> {
        Self::multi_aliases()
            .iter()
            .find(|(alias, _)| *alias == name)
            .map(|(_, aliases)| *aliases)
    }
    /// Try to parse multiple primitives from the concatenation of their name prefixes
    pub fn from_format_name_multi(name: &str) -> Option<Vec<(Self, &str)>> {
        let mut indices: Vec<usize> = name.char_indices().map(|(i, _)| i).collect();
        if indices.len() < 2 {
            return None;
        }
        indices.push(name.len());
        // Forward parsing
        let mut prims = Vec::new();
        let mut start = 0;
        'outer: loop {
            if start == indices.len() {
                return Some(prims);
            }
            let start_index = indices[start];
            for len in (2..=indices.len() - start).rev() {
                let end_index = indices.get(start + len).copied().unwrap_or(name.len());
                if end_index - start_index < 2 {
                    continue;
                }
                let sub_name = &name[start_index..end_index];
                // Normal primitive matching
                if let Some(p) = Primitive::from_format_name(sub_name) {
                    prims.push((p, sub_name));
                    start += len;
                    continue 'outer;
                }
                // 1-letter planet notation
                if sub_name
                    .strip_prefix('f')
                    .unwrap_or(sub_name)
                    .strip_suffix(['i', 'p', 'f'])
                    .unwrap_or(sub_name)
                    .chars()
                    .all(|c| "gd".contains(c))
                    && sub_name != "fi"
                {
                    for (i, c) in sub_name.char_indices() {
                        let prim = match c {
                            'f' if i == 0 => Primitive::Fork,
                            'f' => Primitive::Fix,
                            'g' => Primitive::Gap,
                            'd' => Primitive::Dip,
                            'i' => Primitive::Identity,
                            'p' => Primitive::Pop,
                            _ => unreachable!(),
                        };
                        prims.push((prim, &sub_name[i..i + 1]))
                    }
                    start += len;
                    continue 'outer;
                }
                // Dip fix
                if sub_name
                    .strip_suffix('f')
                    .unwrap_or(sub_name)
                    .chars()
                    .all(|c| c == 'd')
                {
                    for (i, c) in sub_name.char_indices() {
                        let prim = match c {
                            'd' => Primitive::Dip,
                            'f' => Primitive::Fix,
                            _ => unreachable!(),
                        };
                        prims.push((prim, &sub_name[i..i + 1]))
                    }
                    start += len;
                    continue 'outer;
                }
                // Aliases
                if let Some(ps) = Self::get_multi_alias(sub_name) {
                    prims.extend(ps);
                    start += len;
                    continue 'outer;
                }
            }
            break;
        }
        // Backward parsing
        prims.clear();
        let mut end = indices.len() - 1;
        'outer: loop {
            if end == 0 {
                prims.reverse();
                return Some(prims);
            }
            let end_index = indices[end];
            for len in (2..=end).rev() {
                let start_index = indices.get(end - len).copied().unwrap_or(0);
                let sub_name = &name[start_index..end_index];
                // Normal primitive matching
                if let Some(p) = Primitive::from_format_name(sub_name) {
                    prims.push((p, sub_name));
                    end -= len;
                    continue 'outer;
                }
                // 1-letter planet notation
                if sub_name
                    .strip_prefix('f')
                    .unwrap_or(sub_name)
                    .strip_suffix(['i', 'p', 'f'])
                    .unwrap_or(sub_name)
                    .chars()
                    .all(|c| "gd".contains(c))
                    && sub_name != "fi"
                {
                    for (i, c) in sub_name.char_indices().rev() {
                        let prim = match c {
                            'f' if i == 0 => Primitive::Fork,
                            'f' => Primitive::Fix,
                            'g' => Primitive::Gap,
                            'd' => Primitive::Dip,
                            'i' => Primitive::Identity,
                            'p' => Primitive::Pop,
                            _ => unreachable!(),
                        };
                        prims.push((prim, &sub_name[i..i + 1]))
                    }
                    end -= len;
                    continue 'outer;
                }
                // Dip fix
                if sub_name
                    .strip_suffix('f')
                    .unwrap_or(sub_name)
                    .chars()
                    .all(|c| c == 'd')
                {
                    for (i, c) in sub_name.char_indices().rev() {
                        let prim = match c {
                            'd' => Primitive::Dip,
                            'f' => Primitive::Fix,
                            _ => unreachable!(),
                        };
                        prims.push((prim, &sub_name[i..i + 1]))
                    }
                    end -= len;
                    continue 'outer;
                }
                // Aliases
                if let Some(ps) = Self::get_multi_alias(sub_name) {
                    prims.extend(ps.iter().rev());
                    end -= len;
                    continue 'outer;
                }
            }
            break;
        }
        None
    }
    /// Execute the primitive
    pub fn run(&self, env: &mut Uiua) -> UiuaResult {
        match self {
            Primitive::Eta => env.push(eta()),
            Primitive::Pi => env.push(pi()),
            Primitive::Tau => env.push(tau()),
            Primitive::Infinity => env.push(inf()),
            Primitive::Identity => env.touch_stack(1)?,
            Primitive::Not => env.monadic_env(Value::not)?,
            Primitive::Neg => env.monadic_env(Value::neg)?,
            Primitive::Abs => env.monadic_env(Value::abs)?,
            Primitive::Sign => env.monadic_env(Value::sign)?,
            Primitive::Sqrt => env.monadic_env(Value::sqrt)?,
            Primitive::Sin => env.monadic_env(Value::sin)?,
            Primitive::Floor => env.monadic_env(Value::floor)?,
            Primitive::Ceil => env.monadic_env(Value::ceil)?,
            Primitive::Round => env.monadic_env(Value::round)?,
            Primitive::Eq => env.dyadic_oo_env(Value::is_eq)?,
            Primitive::Ne => env.dyadic_oo_env(Value::is_ne)?,
            Primitive::Lt => env.dyadic_oo_env(Value::other_is_lt)?,
            Primitive::Le => env.dyadic_oo_env(Value::other_is_le)?,
            Primitive::Gt => env.dyadic_oo_env(Value::other_is_gt)?,
            Primitive::Ge => env.dyadic_oo_env(Value::other_is_ge)?,
            Primitive::Add => env.dyadic_oo_env(Value::add)?,
            Primitive::Sub => env.dyadic_oo_env(Value::sub)?,
            Primitive::Mul => env.dyadic_oo_env(Value::mul)?,
            Primitive::Div => env.dyadic_oo_env(Value::div)?,
            Primitive::Modulus => env.dyadic_oo_env(Value::modulus)?,
            Primitive::Or => env.dyadic_oo_env(Value::or)?,
            Primitive::Pow => env.dyadic_oo_env(Value::pow)?,
            Primitive::Log => env.dyadic_oo_env(Value::log)?,
            Primitive::Min => env.dyadic_oo_env(Value::min)?,
            Primitive::Max => env.dyadic_oo_env(Value::max)?,
            Primitive::Atan => env.dyadic_oo_env(Value::atan2)?,
            Primitive::Match => env.dyadic_rr(|a, b| a == b)?,
            Primitive::Join => env.dyadic_oo_env(|a, b, env| a.join(b, true, env))?,
            Primitive::Transpose => env.monadic_mut(Value::transpose)?,
            Primitive::Keep => env.dyadic_oo_env(Value::keep)?,
            Primitive::Take => env.dyadic_oo_env(Value::take)?,
            Primitive::Drop => env.dyadic_oo_env(Value::drop)?,
            Primitive::Rotate => {
                let amnt = env.pop(1)?;
                let mut val = env.pop(2)?;
                amnt.rotate(&mut val, env)?;
                env.push(val);
            }
            Primitive::Orient => env.dyadic_ro_env(|a, mut b, env| {
                a.orient(&mut b, env)?;
                Ok(b)
            })?,
            Primitive::Couple => env.dyadic_oo_env(|a, b, env| a.couple(b, true, env))?,
            Primitive::Sort => env.monadic_mut(Value::sort_up)?,
            Primitive::Rise => env.monadic_ref(Value::rise)?,
            Primitive::Fall => env.monadic_ref(Value::fall)?,
            Primitive::Pick => env.dyadic_oo_env(Value::pick)?,
            Primitive::Select => env.dyadic_oo_env(Value::select)?,
            Primitive::Where => env.monadic_ref_env(Value::wher)?,
            Primitive::Classify => env.monadic_ref(Value::classify)?,
            Primitive::Deduplicate => env.monadic_mut_env(Value::deduplicate)?,
            Primitive::Unique => env.monadic_ref(Value::unique)?,
            Primitive::MemberOf => env.dyadic_rr_env(Value::memberof)?,
            Primitive::Find => env.dyadic_rr_env(Value::find)?,
            Primitive::Mask => env.dyadic_rr_env(Value::mask)?,
            Primitive::IndexOf => env.dyadic_rr_env(Value::index_of)?,
            Primitive::Box => {
                let val = env.pop(1)?;
                if val.box_nesting() > 1000 {
                    return Err(env.error("Box nesting too deep"));
                }
                env.push(val.box_depth(0));
            }
            Primitive::Repr => env.monadic_ref(Value::representation)?,
            Primitive::Parse => env.monadic_ref_env(Value::parse_num)?,
            Primitive::Utf8 => env.monadic_ref_env(Value::utf8)?,
            Primitive::Graphemes => env.monadic_ref_env(Value::graphemes)?,
            Primitive::Range => env.monadic_ref_env(Value::range)?,
            Primitive::Reverse => env.monadic_mut(Value::reverse)?,
            Primitive::Deshape => env.monadic_mut(Value::deshape)?,
            Primitive::Fix => env.monadic_mut(Value::fix)?,
            Primitive::First => env.monadic_env(Value::first)?,
            Primitive::Last => env.monadic_env(Value::last)?,
            Primitive::Len => env.monadic_ref(Value::row_count)?,
            Primitive::Shape => {
                env.monadic_ref(|v| v.shape().iter().copied().collect::<Value>())?
            }
            Primitive::Bits => env.monadic_ref_env(|val, env| val.bits(None, env))?,
            Primitive::Base => env.dyadic_rr_env(Value::base)?,
            Primitive::Reshape => {
                let shape = env.pop(1)?;
                let mut array = env.pop(2)?;
                array.reshape(&shape, env)?;
                env.push(array);
            }
            Primitive::Rerank => {
                let rank = env.pop(1)?;
                let mut array = env.pop(2)?;
                array.rerank(&rank, env)?;
                env.push(array);
            }
            Primitive::Dup => {
                let x = env.pop(1)?;
                env.push(x.clone());
                env.push(x);
            }
            Primitive::Flip => {
                let a = env.pop(1)?;
                let b = env.pop(2)?;
                env.push(a);
                env.push(b);
            }
            Primitive::Over => {
                let a = env.pop(1)?;
                let b = env.pop(2)?;
                env.push(b.clone());
                env.push(a);
                env.push(b);
            }
            Primitive::Around => {
                let a = env.pop(1)?;
                let b = env.pop(2)?;
                env.push(a.clone());
                env.push(b);
                env.push(a);
            }
            Primitive::Pop => {
                env.pop(1)?;
            }
            Primitive::Assert => {
                let msg = env.pop(1)?;
                let cond = env.pop(2)?;
                if !cond.as_nat(env, "").is_ok_and(|n| n == 1) {
                    return Err(UiuaErrorKind::Throw(
                        msg.into(),
                        env.span().clone(),
                        env.asm.inputs.clone().into(),
                    )
                    .into());
                }
            }
            Primitive::Rand => env.push(randf() as f64),
            Primitive::Gen => env.dyadic_rr_env(Value::gen)?,
            Primitive::Tag => {
                panic!("no tag");
            }
            Primitive::Type => {
                let val = env.pop(1)?;
                env.push(val.type_id());
            }
            Primitive::Wait |
            Primitive::Send |
            Primitive::Recv |
            Primitive::TryRecv => {
                panic!("no");
            }
            Primitive::Now => env.push(env.rt.backend.now()),
            Primitive::TimeZone => {
                let o = env.rt.backend.timezone().map_err(|e| env.error(e))?;
                env.push(o);
            }
            Primitive::DateTime => env.monadic_ref_env(Value::datetime)?,
            Primitive::Insert => {
                let key = env.pop("key")?;
                let val = env.pop("value")?;
                let mut map = env.pop("map")?;
                map.insert(key, val, env)?;
                env.push(map);
            }
            Primitive::Has => {
                let key = env.pop("key")?;
                let map = env.pop("map")?;
                env.push(map.has_key(&key, env)?);
            }
            Primitive::Get => {
                let key = env.pop("key")?;
                let map = env.pop("map")?;
                let val = map.get(&key, env)?;
                env.push(val);
            }
            Primitive::Remove => {
                let key = env.pop("key")?;
                let mut map = env.pop("map")?;
                map.remove(key, env)?;
                env.push(map);
            }
            Primitive::Map => {
                let keys = env.pop("keys")?;
                let mut vals = env.pop("values")?;
                vals.map(keys, env)?;
                env.push(vals);
            }
            Primitive::Trace => trace(env, false)?,
            Primitive::Stack => stack(env, false)?,
            Primitive::Stringify
            | Primitive::Quote
            | Primitive::Sig
            | Primitive::Comptime
            | Primitive::Un
            | Primitive::Anti
            | Primitive::Under
            | Primitive::Obverse
            | Primitive::Switch => {
                return Err(env.error(format!(
                    "{} was not inlined. This is a bug in the interpreter",
                    self.format()
                )))
            }
            Primitive::Sys(io) => io.run(env)?,
            prim => {
                return Err(env.error(if prim.modifier_args().is_some() {
                    format!(
                        "{} was not handled as a modifier. \
                        This is a bug in the interpreter",
                        prim.format()
                    )
                } else {
                    format!(
                        "{} was not handled as a function. \
                        This is a bug in the interpreter",
                        prim.format()
                    )
                }))
            }
        }
        Ok(())
    }
    /// Run a primitive as a modifier
    pub fn run_mod(&self, ops: Ops, env: &mut Uiua) -> UiuaResult {
        match self {
            // Looping
            Primitive::Reduce => reduce::reduce(ops, 0, env)?,
            Primitive::Scan => reduce::scan(ops, env)?,
            Primitive::Fold => reduce::fold(ops, env)?,
            Primitive::Each => zip::each(ops, env)?,
            Primitive::Rows => {
                let [f] = get_ops(ops, env)?;
                zip::rows(f, false, env)?
            }
            Primitive::Inventory => {
                let [f] = get_ops(ops, env)?;
                zip::rows(f, true, env)?
            }
            Primitive::Table => table::table(ops, env)?,
            Primitive::Repeat => loops::repeat(ops, false, false, env)?,
            Primitive::Do => loops::do_(ops, env)?,
            Primitive::Group => {
                let [f] = get_ops(ops, env)?;
                loops::group(f, env)?
            }
            Primitive::Partition => {
                let [f] = get_ops(ops, env)?;
                loops::partition(f, env)?
            }
            Primitive::Tuples => tuples::tuples(ops, env)?,
            Primitive::Stencil => stencil::stencil(ops, env)?,

            // Stack
            Primitive::Fork => {
                let [f, g] = get_ops(ops, env)?;
                let f_args = env.prepare_fork(f.sig.args, g.sig.args)?;
                env.exec(g)?;
                env.push_all(f_args);
                env.exec(f)?;
            }
            Primitive::Bracket => {
                let [f, g] = get_ops(ops, env)?;
                let vals = env.take_n(f.sig.args)?;
                env.exec(g)?;
                env.push_all(vals);
                env.exec(f)?;
            }
            Primitive::Both => {
                let [f] = get_ops(ops, env)?;
                let vals = env.take_n(f.sig.args)?;
                env.exec(f.node.clone())?;
                env.push_all(vals);
                env.exec(f.node)?;
            }
            Primitive::Dip => {
                let [f] = get_ops(ops, env)?;
                let val = env.pop(1)?;
                env.exec(f)?;
                env.push(val);
            }
            Primitive::On => {
                let [f] = get_ops(ops, env)?;
                let val = env.copy_nth(0)?;
                env.exec(f)?;
                env.push(val);
            }
            Primitive::By => {
                let [f] = get_ops(ops, env)?;
                env.dup_values(1, f.sig.args.max(1))?;
                env.exec(f)?;
            }
            Primitive::Above => {
                let [f] = get_ops(ops, env)?;
                let vals = env.copy_n(f.sig.args)?;
                env.exec(f)?;
                env.push_all(vals);
            }
            Primitive::Below => {
                let [f] = get_ops(ops, env)?;
                env.dup_values(f.sig.args, f.sig.args)?;
                env.exec(f)?;
            }
            Primitive::With => {
                let [f] = get_ops(ops, env)?;
                let val = env.copy_nth(f.sig.args - 1)?;
                env.exec(f)?;
                env.push(val);
            }
            Primitive::Off => {
                let [f] = get_ops(ops, env)?;
                let val = env.copy_nth(0)?;
                env.exec(f.node)?;
                env.push(val);
                env.rotate_up(1, f.sig.outputs + 1)?;
            }
            Primitive::Content => {
                let [f] = get_ops(ops, env)?;
                for val in env.n_mut(f.sig.args)? {
                    val.unbox();
                }
                env.exec(f)?;
            }

            // Misc
            Primitive::Fill => fill!(ops, env, with_fill, without_fill_but),
            Primitive::Try => algorithm::try_(ops, env)?,
            Primitive::Case => {
                let [f] = get_ops(ops, env)?;
                env.exec(f).map_err(|mut e| {
                    e.is_case = true;
                    e
                })?;
            }
            Primitive::Dump => dump(ops, env, false)?,
            Primitive::Astar => {
                let [neighbors, heuristic, is_goal] = get_ops(ops, env)?;
                path::path(neighbors, is_goal, Some(heuristic), env)?;
            }
            Primitive::Path => {
                let [neighbors, is_goal] = get_ops(ops, env)?;
                path::path(neighbors, is_goal, None, env)?;
            }
            Primitive::Memo => {
                panic!("no");
            }
            Primitive::Spawn => {
                panic!("no");
            }
            Primitive::Pool => {
                panic!("no");
            }
            Primitive::Sys(op) => op.run_mod(ops, env)?,
            prim => {
                return Err(env.error(if prim.modifier_args().is_some() {
                    format!(
                        "{} was not handled as a modifier. \
                        This is a bug in the interpreter",
                        prim.format()
                    )
                } else {
                    format!(
                        "{} was called as a modifier. \
                        This is a bug in the interpreter",
                        prim.format()
                    )
                }))
            }
        }
        Ok(())
    }
}

impl ImplPrimitive {
    pub(crate) fn run(&self, env: &mut Uiua) -> UiuaResult {
        match self {
            ImplPrimitive::DeshapeSub(i) => {
                env.monadic_mut_env(|val, env| val.deshape_sub(*i, true, env))?
            }
            ImplPrimitive::NBits(n) => {
                let val = env.pop(1)?;
                let bits = val.bits(Some(*n), env)?;
                env.push(bits);
            }
            ImplPrimitive::Root => env.dyadic_oo_env(Value::root)?,
            ImplPrimitive::Cos => env.monadic_env(Value::cos)?,
            ImplPrimitive::Asin => env.monadic_env(Value::asin)?,
            ImplPrimitive::Acos => env.monadic_env(Value::acos)?,
            ImplPrimitive::UnPop => {
                let val = (env.last_fill()).ok_or_else(|| env.error("No fill set").fill())?;
                env.push(val.clone());
            }
            ImplPrimitive::UnCouple => {
                let coupled = env.pop(1)?;
                let (a, b) = coupled.uncouple(env)?;
                env.push(b);
                env.push(a);
            }
            ImplPrimitive::UnMap => {
                let map = env.pop(1)?;
                let (keys, vals) = map.unmap(env)?;
                env.push(vals);
                env.push(keys);
            }
            ImplPrimitive::UnWhere => env.monadic_ref_env(Value::unwhere)?,
            ImplPrimitive::Utf16 => env.monadic_ref_env(Value::utf16)?,
            ImplPrimitive::UnUtf8 => env.monadic_ref_env(Value::unutf8)?,
            ImplPrimitive::UnUtf16 => env.monadic_ref_env(Value::unutf16)?,
            ImplPrimitive::UnGraphemes => env.monadic_env(Value::ungraphemes)?,
            ImplPrimitive::UnBits => env.monadic_ref_env(Value::unbits)?,
            ImplPrimitive::AntiDrop => env.dyadic_ro_env(Value::anti_drop)?,
            ImplPrimitive::AntiSelect => env.dyadic_oo_env(Value::anti_select)?,
            ImplPrimitive::AntiPick => env.dyadic_oo_env(Value::anti_pick)?,
            ImplPrimitive::UnJoin => {
                let val = env.pop(1)?;
                let (first, rest) = val.unjoin(env)?;
                env.push(rest);
                env.push(first);
            }
            ImplPrimitive::UnJoinEnd => {
                let val = env.pop(1)?;
                let (first, rest) = val.unjoin_end(env)?;
                env.push(rest);
                env.push(first);
            }
            ImplPrimitive::UnJoinShape => {
                let shape = (env.pop(1))?.as_nats(env, "Shape must be natural numbers")?;
                let val = env.pop(2)?;
                let (first, rest) = val.unjoin_shape(&shape, None, false, env)?;
                env.push(rest);
                env.push(first);
            }
            ImplPrimitive::UnJoinShapeEnd => {
                let shape = (env.pop(1))?.as_nats(env, "Shape must be natural numbers")?;
                let val = env.pop(2)?;
                let (first, rest) = val.unjoin_shape(&shape, None, true, env)?;
                env.push(rest);
                env.push(first);
            }
            ImplPrimitive::UnJoinShape2 => {
                let a_shape = (env.pop(1))?.as_nats(env, "Shape must be natural numbers")?;
                let b_shape = (env.pop(1))?.as_nats(env, "Shape must be natural numbers")?;
                let val = env.pop(2)?;
                let (first, rest) = val.unjoin_shape(&a_shape, Some(&b_shape), false, env)?;
                env.push(rest);
                env.push(first);
            }
            ImplPrimitive::UnJoinShape2End => {
                let a_shape = (env.pop(1))?.as_nats(env, "Shape must be natural numbers")?;
                let b_shape = (env.pop(1))?.as_nats(env, "Shape must be natural numbers")?;
                let val = env.pop(2)?;
                let (first, rest) = val.unjoin_shape(&a_shape, Some(&b_shape), true, env)?;
                env.push(rest);
                env.push(first);
            }
            ImplPrimitive::UnKeep => {
                let val = env.pop(1)?;
                let (counts, dedup) = val.unkeep(env)?;
                env.push(dedup);
                env.push(counts);
            }
            ImplPrimitive::UnAtan => {
                let x = env.pop(1)?;
                let sin = x.clone().sin(env)?;
                let cos = x.cos(env)?;
                env.push(cos);
                env.push(sin);
            }
            ImplPrimitive::UnParse => env.monadic_ref_env(Value::unparse)?,
            ImplPrimitive::UnFix => env.monadic_mut_env(Value::unfix)?,
            ImplPrimitive::UnShape => env.monadic_ref_env(Value::unshape)?,
            ImplPrimitive::StackN { n, inverse } => stack_n(env, *n, *inverse)?,
            ImplPrimitive::UnStack => stack(env, true)?,
            ImplPrimitive::Primes => env.monadic_ref_env(Value::primes)?,
            ImplPrimitive::UnBox => {
                let val = env.pop(1)?;
                env.push(val.unboxed());
            }
            ImplPrimitive::UnSort => {
                let arr = env.pop(1)?;
                if arr.row_count() < 2 {
                    env.push(arr);
                } else {
                    let mut rows: Vec<Value> = arr.into_rows().collect();
                    shuffle(&mut rows);
                    env.push(Value::from_row_values_infallible(rows));
                }
            }
            ImplPrimitive::UnDatetime => env.monadic_ref_env(Value::undatetime)?,
            ImplPrimitive::ProgressiveIndexOf => env.dyadic_rr_env(Value::progressive_index_of)?,
            ImplPrimitive::MatrixDiv => env.dyadic_rr_env(Value::matrix_div)?,
            // Unders
            ImplPrimitive::UndoUnBits => {
                let orig_shape = env.pop(1)?;
                let val = env.pop(2)?;
                env.push(val.undo_un_bits(&orig_shape, env)?);
            }
            ImplPrimitive::AntiBase => env.dyadic_rr_env(Value::antibase)?,
            &ImplPrimitive::UndoReverse { n, all } => {
                env.require_height(n)?;
                let end = env.stack_height() - n;
                let vals = &mut env.stack_mut()[end..];
                if all {
                    for val in vals {
                        val.reverse();
                    }
                } else {
                    let max_rank = vals.iter().map(|v| v.rank()).max().unwrap_or(0);
                    for val in vals {
                        if val.rank() == max_rank {
                            val.reverse();
                        }
                    }
                }
            }
            &ImplPrimitive::UndoTransposeN(n, amnt) => {
                env.touch_stack(n)?;
                let end = env.stack_height() - n;
                let vals = &mut env.stack_mut()[end..];
                let max_rank = vals.iter().map(|v| v.rank()).max().unwrap_or(0);
                for val in vals {
                    if val.rank() == max_rank {
                        val.transpose_depth(0, -amnt);
                    }
                }
            }
            &ImplPrimitive::UndoRotate(n) => {
                env.touch_stack(n + 1)?;
                let mut amount = env.pop(1)?.scalar_neg(env)?;
                if n == 1 {
                    let mut val = env.pop(2)?;
                    if amount.rank() > 0 && amount.row_count() > val.rank() {
                        amount.drop_n(amount.row_count() - val.rank());
                    }
                    amount.rotate(&mut val, env)?;
                    env.push(val);
                } else {
                    let end = env.stack_height() - n;
                    let mut vals = env.truncate_stack(end);
                    let max_rank = vals.iter().map(|v| v.rank()).max().unwrap_or(0);
                    if amount.rank() > 0 && amount.row_count() > max_rank {
                        amount.drop_n(amount.row_count() - max_rank);
                    }
                    for val in &mut vals {
                        let mut amount = amount.clone();
                        if amount.row_count() > 1 {
                            if amount.rank() > 0 && amount.row_count() > val.rank() {
                                amount.drop_n(amount.row_count() - val.rank());
                            }
                            amount.rotate(val, env)?;
                        } else if val.rank() == max_rank {
                            amount.rotate(val, env)?;
                        }
                    }
                    for val in vals {
                        env.push(val);
                    }
                }
            }
            ImplPrimitive::UndoPick => {
                let index = env.pop(1)?;
                let into = env.pop(2)?;
                let from = env.pop(3)?;
                env.push(from.undo_pick(index, into, env)?);
            }
            ImplPrimitive::UndoSelect => {
                let index = env.pop(1)?;
                let into = env.pop(2)?;
                let from = env.pop(3)?;
                env.push(from.undo_select(index, into, env)?);
            }
            ImplPrimitive::UndoWhere => {
                let shape = env.pop(1)?.as_nats(env, "Shape must be natural numbers")?;
                let indices = env.pop(2)?;
                let mask = indices.undo_where(&shape, env)?;
                env.push(mask);
            }
            ImplPrimitive::AntiOrient => env.dyadic_ro_env(Value::anti_orient)?,
            ImplPrimitive::UndoAntiOrient => {
                let indices = env.pop(1)?;
                let into = env.pop(2)?;
                let from = env.pop(3)?;
                env.push(from.undo_anti_orient(indices, into, env)?);
            }
            ImplPrimitive::UndoRerank => {
                let rank = env.pop(1)?;
                let shape = Shape::from(
                    env.pop(2)?
                        .as_nats(env, "Shape must be a list of natural numbers")?,
                );
                let mut array = env.pop(3)?;
                array.undo_rerank(&rank, &shape, env)?;
                env.push(array);
            }
            ImplPrimitive::UndoReshape => env.dyadic_ro_env(|orig_shape, mut val, env| {
                val.undo_reshape(orig_shape, env)?;
                Ok(val)
            })?,
            ImplPrimitive::UndoWindows => env.dyadic_ro_env(Value::undo_windows)?,
            ImplPrimitive::UndoFirst => {
                let into = env.pop(1)?;
                let from = env.pop(2)?;
                env.push(from.undo_first(into, env)?);
            }
            ImplPrimitive::UndoLast => {
                let into = env.pop(1)?;
                let from = env.pop(2)?;
                env.push(from.undo_last(into, env)?);
            }
            ImplPrimitive::UndoKeep => {
                let from = env.pop(1)?;
                let counts = env.pop(2)?;
                let into = env.pop(3)?;
                env.push(from.undo_keep(counts, into, env)?);
            }
            ImplPrimitive::UndoTake => {
                let index = env.pop(1)?;
                let into = env.pop(2)?;
                let from = env.pop(3)?;
                env.push(from.undo_take(index, into, env)?);
            }
            ImplPrimitive::UndoDrop => {
                let index = env.pop(1)?;
                let into = env.pop(2)?;
                let from = env.pop(3)?;
                env.push(from.undo_drop(index, into, env)?);
            }
            ImplPrimitive::UndoFix => env.monadic_mut(Value::undo_fix)?,
            ImplPrimitive::UndoDeshape(sub) => {
                let shape = Shape::from(
                    env.pop(1)?
                        .as_nats(env, "Shape must be a list of natural numbers")?,
                );
                let mut val = env.pop(2)?;
                val.undo_deshape(*sub, &shape, env)?;
                env.push(val)
            }
            ImplPrimitive::UndoPartition2 => loops::undo_partition_part2(env)?,
            ImplPrimitive::UndoGroup2 => loops::undo_group_part2(env)?,
            ImplPrimitive::UndoJoin => {
                let a_shape = env.pop(1)?;
                let b_shape = env.pop(2)?;
                let val = env.pop(3)?;
                let (left, right) = val.undo_join(a_shape, b_shape, env)?;
                env.push(right);
                env.push(left);
            }
            ImplPrimitive::TryClose => _ = SysOp::Close.run(env),
            ImplPrimitive::UndoInsert => {
                let key = env.pop(1)?;
                let _value = env.pop(2)?;
                let original = env.pop(3)?;
                let mut map = env.pop(4)?;
                map.undo_insert(key, &original, env)?;
                env.push(map);
            }
            ImplPrimitive::UndoRemove => {
                let key = env.pop(1)?;
                let original = env.pop(2)?;
                let mut map = env.pop(3)?;
                map.undo_remove(key, &original, env)?;
                env.push(map);
            }
            &ImplPrimitive::MaxRowCount(n) => {
                let mut max_len: Option<usize> = None;
                let start = env.require_height(n)?;
                for val in &env.stack()[start..] {
                    if val.row_count() != 1 {
                        max_len = Some(max_len.unwrap_or(0).max(val.row_count()));
                    }
                }
                env.push(max_len.unwrap_or(1));
            }
            ImplPrimitive::SetSign => env.dyadic_oo_env(Value::set_sign)?,
            // Optimizations
            ImplPrimitive::FirstMinIndex => env.monadic_ref_env(Value::first_min_index)?,
            ImplPrimitive::FirstMaxIndex => env.monadic_ref_env(Value::first_max_index)?,
            ImplPrimitive::LastMinIndex => env.monadic_ref_env(Value::last_min_index)?,
            ImplPrimitive::LastMaxIndex => env.monadic_ref_env(Value::last_max_index)?,
            ImplPrimitive::FirstWhere => env.monadic_ref_env(Value::first_where)?,
            ImplPrimitive::LenWhere => env.monadic_ref_env(Value::len_where)?,
            ImplPrimitive::MemberOfRange => env.dyadic_ro_env(Value::memberof_range)?,
            ImplPrimitive::MultidimMemberOfRange => {
                env.dyadic_ro_env(Value::multidim_memberof_range)?
            }
            ImplPrimitive::RandomRow => env.monadic_ref_env(Value::random_row)?,
            ImplPrimitive::LastWhere => env.monadic_ref_env(Value::last_where)?,
            ImplPrimitive::SortDown => env.monadic_mut(Value::sort_down)?,
            ImplPrimitive::AllSame => env.monadic_ref(Value::all_same)?,
            ImplPrimitive::ReplaceRand => {
                env.pop(1)?;
                env.push(randf() as f64);
            }
            ImplPrimitive::ReplaceRand2 => {
                env.pop(1)?;
                env.pop(2)?;
                env.push(randf() as f64);
            }
            ImplPrimitive::CountUnique => env.monadic_ref(Value::count_unique)?,
            ImplPrimitive::MatchPattern => {
                let expected = env.pop(1)?;
                let got = env.pop(2)?;
                match (&expected, &got) {
                    (Value::Num(a), Value::Num(b))
                        if a.shape() == b.shape()
                            && (a.data.iter().zip(&b.data)).all(|(a, b)| (a - b).abs() < 1e-12) =>
                    {
                        return Ok(())
                    }
                    (a, b) if a == b => return Ok(()),
                    _ => {}
                }
                let message = match (
                    expected.rank() <= 1 && expected.row_count() <= 10,
                    got.rank() <= 1 && got.row_count() <= 10,
                ) {
                    (true, true) => format!(
                        "expected {} but got {}",
                        expected.grid_string(false),
                        got.grid_string(false)
                    ),
                    (true, false) if expected.type_id() != got.type_id() => {
                        format!(
                            "expected {} but got {}",
                            expected.grid_string(false),
                            got.type_name_plural()
                        )
                    }
                    (true, false) if expected.shape() != got.shape() => format!(
                        "expected {} but got array with shape {}",
                        expected.grid_string(false),
                        got.shape()
                    ),
                    (true, false) => format!(
                        "expected {} but found {} array with shape {}",
                        expected.grid_string(false),
                        got.type_name(),
                        got.shape()
                    ),
                    (false, true) if expected.type_id() != got.type_id() => {
                        format!(
                            "expected {} but got {}",
                            expected.type_name_plural(),
                            got.grid_string(false)
                        )
                    }
                    (false, true) if expected.shape() != got.shape() => format!(
                        "expected array with shape {} but got {}",
                        expected.shape(),
                        got.grid_string(false)
                    ),
                    (false, true) => format!(
                        "expected {} array with shape {} but got {}",
                        expected.type_name(),
                        expected.shape(),
                        got.grid_string(false)
                    ),
                    (false, false) if expected.type_id() != got.type_id() => {
                        format!(
                            "expected {} but got {}",
                            expected.type_name_plural(),
                            got.type_name_plural()
                        )
                    }
                    (false, false) if expected.shape() != got.shape() => format!(
                        "expected shape {} but got shape {}",
                        expected.shape(),
                        got.shape()
                    ),
                    (false, false) => {
                        let different = if expected.type_id() == got.type_id()
                            && expected.shape() == got.shape()
                        {
                            " different"
                        } else {
                            ""
                        };
                        format!(
                            "expected {} array with shape {} but \
                            got{different} {} array with shape {}",
                            expected.type_name(),
                            expected.shape(),
                            got.type_name(),
                            got.shape()
                        )
                    }
                };
                return Err(env.error(env.error(format!("Pattern match failed: {message}"))));
            }
            ImplPrimitive::MatchLe => {
                let max = env.pop(1)?;
                let val = env.pop(2)?;
                let le = max.clone().other_is_le(val.clone(), env)?;
                if le.all_true() {
                    env.push(val);
                    return Ok(());
                }
                let message = if max.rank() <= 1 && max.row_count() <= 10 {
                    format!("Not all values are {} {max}", Primitive::Le)
                } else {
                    format!("Not all values are {}", Primitive::Le)
                };
                return Err(env.error(env.error(format!("Pattern match failed: {message}"))));
            }
            ImplPrimitive::MatchGe => {
                let min = env.pop(1)?;
                let val = env.pop(2)?;
                let ge = min.clone().other_is_ge(val.clone(), env)?;
                if ge.all_true() {
                    env.push(val);
                    return Ok(());
                }
                let message = if min.rank() <= 1 && min.row_count() <= 10 {
                    format!("Not all values are {} {min}", Primitive::Ge)
                } else {
                    format!("Not all values are {}", Primitive::Ge)
                };
                return Err(env.error(env.error(format!("Pattern match failed: {message}"))));
            }
            &ImplPrimitive::TransposeN(n) => env.monadic_mut(|val| val.transpose_depth(0, n))?,
            // Implementation details
            ImplPrimitive::ValidateType | ImplPrimitive::ValidateTypeConsume => {
                let type_num = env
                    .pop(1)?
                    .as_nat(env, "Type number must be a natural number")?;
                let val = env.pop(2)?;
                if val.type_id() as usize != type_num {
                    let found = if val.element_count() == 1 {
                        val.type_name()
                    } else {
                        val.type_name_plural()
                    };
                    let expected = match type_num {
                        0 => "numbers",
                        1 => "characters",
                        2 => "boxes",
                        3 => "complex numbers",
                        _ => return Err(env.error(format!("Invalid type number {type_num}"))),
                    };
                    return Err(env.error(format!("Expected {expected} but found {found}")));
                }
                if let ImplPrimitive::ValidateType = self {
                    env.push(val);
                }
            }
            ImplPrimitive::TestAssert => {
                let msg = env.pop(1)?;
                let cond = env.pop(2)?;
                let mut res = Ok(());
                if !cond.as_nat(env, "").is_ok_and(|n| n == 1) {
                    res = Err(UiuaErrorKind::Throw(
                        msg.into(),
                        env.span().clone(),
                        env.asm.inputs.clone().into(),
                    )
                    .into());
                }
                env.rt.test_results.push(res);
            }
            ImplPrimitive::ValidateNonBoxedVariant => {
                let val = env.pop(1)?;
                if !matches!(val, Value::Num(_) | Value::Byte(_) | Value::Box(_)) {
                    return Err(env.error(format!(
                        "Non-boxed variant field must be numbers or boxes, but it is {}",
                        val.type_name_plural()
                    )));
                }
                if val.rank() > 0 {
                    return Err(env.error(format!(
                        "Non-boxed variant field must be rank 0 or 1, but it is rank {}",
                        val.rank()
                    )));
                }
                env.push(val);
            }
            ImplPrimitive::ValidateVariant => {
                let tag = env.pop(1)?;
                let val = env.pop(2)?;
                if val.row_count() == 0 {
                    return Err(env.error("Variant must have at least one row"));
                }
                if val.rank() == 0 {
                    return Err(env.error(format!("Variant tag is {val} instead of {tag}")));
                }
                let (head, tail) = val.unjoin(env).unwrap();
                let set_tag = head.unboxed();
                if tag != set_tag {
                    return Err(env.error(if set_tag.rank() == 0 {
                        format!("Variant tag is {set_tag} instead of {tag}")
                    } else {
                        format!("Variant tag is rank {} instead of {tag}", set_tag.rank())
                    }));
                }
                env.push(tail);
            }
            ImplPrimitive::TagVariant => {
                let mut tag = env.pop(1)?;
                let val = env.pop(2)?;
                if let Value::Box(_) = &val {
                    tag.box_if_not();
                }
                let res = tag.join(val, false, env)?;
                env.push(res);
            }
            prim => {
                return Err(env.error(if prim.modifier_args().is_some() {
                    format!(
                        "{prim} was handled as a function. \
                        This is a bug in the interpreter"
                    )
                } else {
                    format!(
                        "{prim} was not handled as a function. \
                        This is a bug in the interpreter"
                    )
                }))
            }
        }
        Ok(())
    }
    pub(crate) fn run_mod(&self, ops: Ops, env: &mut Uiua) -> UiuaResult {
        match self {
            &ImplPrimitive::OnSub(n) => {
                let [f] = get_ops(ops, env)?;
                let kept = env.copy_n(n)?;
                env.exec(f)?;
                env.push_all(kept);
            }
            &ImplPrimitive::BySub(n) => {
                let [f] = get_ops(ops, env)?;
                env.dup_values(n, n.max(f.sig.args))?;
                env.exec(f)?;
            }
            &ImplPrimitive::WithSub(n) => {
                let [f] = get_ops(ops, env)?;
                let kept = env.copy_n_down(n, n.max(f.sig.args))?;
                env.exec(f)?;
                env.push_all(kept);
            }
            &ImplPrimitive::OffSub(n) => {
                let [f] = get_ops(ops, env)?;
                let outputs = f.sig.outputs;
                let kept = env.copy_n(n)?;
                env.exec(f)?;
                env.insert_stack(outputs, kept)?;
            }
            ImplPrimitive::UndoPartition1 => loops::undo_partition_part1(ops, env)?,
            ImplPrimitive::UndoGroup1 => loops::undo_group_part1(ops, env)?,
            ImplPrimitive::ReduceContent => reduce::reduce_content(ops, env)?,
            ImplPrimitive::ReduceConjoinInventory => zip::reduce_conjoin_inventory(ops, env)?,
            ImplPrimitive::AstarFirst => {
                let [neighbors, heuristic, is_goal] = get_ops(ops, env)?;
                path::path_first(neighbors, is_goal, Some(heuristic), env)?;
            }
            ImplPrimitive::AstarTake => {
                let [neighbors, heuristic, is_goal] = get_ops(ops, env)?;
                path::path_take(neighbors, is_goal, Some(heuristic), env)?;
            }
            ImplPrimitive::AstarPop => {
                let [neighbors, heuristic, is_goal] = get_ops(ops, env)?;
                path::path_pop(neighbors, is_goal, Some(heuristic), env)?;
            }
            ImplPrimitive::PathFirst => {
                let [neighbors, is_goal] = get_ops(ops, env)?;
                path::path_first(neighbors, is_goal, None, env)?;
            }
            ImplPrimitive::PathTake => {
                let [neighbors, is_goal] = get_ops(ops, env)?;
                path::path_take(neighbors, is_goal, None, env)?;
            }
            ImplPrimitive::PathPop => {
                let [neighbors, is_goal] = get_ops(ops, env)?;
                path::path_pop(neighbors, is_goal, None, env)?;
            }
            &ImplPrimitive::ReduceDepth(depth) => reduce::reduce(ops, depth, env)?,
            ImplPrimitive::RepeatWithInverse => loops::repeat(ops, true, false, env)?,
            ImplPrimitive::RepeatCountConvergence => loops::repeat(ops, false, true, env)?,
            ImplPrimitive::UnScan => reduce::unscan(ops, env)?,
            ImplPrimitive::UnDump => dump(ops, env, true)?,
            ImplPrimitive::UnFill => fill!(ops, env, with_unfill, without_unfill_but),
            ImplPrimitive::ReduceTable => table::reduce_table(ops, env)?,
            ImplPrimitive::UnBoth => {
                let [f] = get_ops(ops, env)?;
                env.exec(f.node.clone())?;
                let vals = env.take_n(f.sig.outputs)?;
                env.exec(f.node)?;
                env.push_all(vals);
            }
            ImplPrimitive::UnBracket => {
                let [f, g] = get_ops(ops, env)?;
                env.exec(f.node)?;
                let f_outputs = env.pop_n(f.sig.outputs)?;
                env.exec(g.node)?;
                env.push_all(f_outputs);
            }
            ImplPrimitive::SplitByScalar => {
                let [f] = get_ops(ops, env)?;
                loops::split_by(f, true, false, env)?;
            }
            ImplPrimitive::SplitBy => {
                let [f] = get_ops(ops, env)?;
                loops::split_by(f, false, false, env)?;
            }
            ImplPrimitive::SplitByKeepEmpty => {
                let [f] = get_ops(ops, env)?;
                loops::split_by(f, false, true, env)?;
            }
            &ImplPrimitive::EachSub(n) => {
                let [f] = get_ops(ops, env)?;
                let sig = f.sig;
                let vals = env.pop_n(sig.args)?;
                let max_shape = vals
                    .iter()
                    .map(Value::shape)
                    .max_by_key(|sh| sh.len())
                    .cloned();
                let max_rank = max_shape.as_ref().map(|sh| sh.len()).unwrap_or(0);
                for mut val in vals {
                    val.deshape_sub(n + 1, val.rank() == max_rank, env)?;
                    env.push(val);
                }
                zip::rows(f, false, env)?;
                for mut value in env.pop_n(sig.outputs)? {
                    if let Some(max_shape) = &max_shape {
                        value.undo_deshape(Some(n + 1), max_shape, env)?;
                    }
                    env.push(value);
                }
            }
            ImplPrimitive::UndoRows | ImplPrimitive::UndoInventory => {
                let [f] = get_ops(ops, env)?;
                let len = env
                    .pop(1)?
                    .as_nat(env, "Rows length must be a natural number")?;
                let start = env.require_height(f.sig.args)?;
                let inventory = matches!(self, ImplPrimitive::UndoInventory);
                for i in 0..f.sig.args {
                    let val = &env.stack()[start + i];
                    if val.row_count() != len {
                        return Err(env.error(format!(
                            "Cannot undo {} of length {len} when \
                            transformed array has shape {}",
                            if inventory {
                                Primitive::Inventory
                            } else {
                                Primitive::Rows
                            }
                            .format(),
                            val.shape()
                        )));
                    }
                    env.stack_mut()[start + i].reverse();
                }
                let outputs = f.sig.outputs;
                zip::rows(f, inventory, env)?;
                let start = env.require_height(outputs)?;
                for val in &mut env.stack_mut()[start..] {
                    val.reverse();
                }
            }
            prim => {
                return Err(env.error(if prim.modifier_args().is_some() {
                    format!(
                        "{prim} was not handled as a modifier. \
                        This is a bug in the interpreter"
                    )
                } else {
                    format!(
                        "{prim} was handled as a modifier. \
                        This is a bug in the interpreter"
                    )
                }))
            }
        }
        Ok(())
    }
}

fn trace(env: &mut Uiua, inverse: bool) -> UiuaResult {
    let val = env.pop(1)?;
    let span: String = if inverse {
        format!("{}{} {}", Primitive::Un, Primitive::Trace, env.span())
    } else {
        format!("{} {}", Primitive::Trace, env.span())
    };
    let max_line_len = span.chars().count() + 2;
    let item_lines =
        format_trace_item_lines(val.show().lines().map(Into::into).collect(), max_line_len);
    env.push(val);
    env.rt.backend.print_str_trace(&format!("┌╴{span}\n"));
    for line in item_lines {
        env.rt.backend.print_str_trace(&line);
    }
    env.rt.backend.print_str_trace("└");
    for _ in 0..max_line_len - 1 {
        env.rt.backend.print_str_trace("╴");
    }
    env.rt.backend.print_str_trace("\n");
    Ok(())
}

fn stack_n(env: &mut Uiua, n: usize, inverse: bool) -> UiuaResult {
    env.require_height(n)?;
    let boundaries = stack_boundaries(env);
    let span = format!("{} {}", ImplPrimitive::StackN { n, inverse }, env.span());
    let max_line_len = span.chars().count() + 2;
    let stack_height = env.stack_height() - n;
    let item_lines: Vec<Vec<String>> = env.stack()[stack_height..]
        .iter()
        .map(Value::show)
        .map(|s| s.lines().map(Into::into).collect::<Vec<String>>())
        .map(|lines| format_trace_item_lines(lines, max_line_len))
        .enumerate()
        .flat_map(|(i, lines)| {
            if let Some((_, id)) = boundaries
                .iter()
                .find(|(height, _)| i + stack_height == *height)
            {
                let id = id.as_ref().map_or_else(String::new, ToString::to_string);
                vec![vec![format!("│╴╴╴{id}╶╶╶\n")], lines]
            } else {
                vec![lines]
            }
        })
        .collect();
    env.rt.backend.print_str_trace(&format!("┌╴{span}\n"));
    for line in item_lines.iter().flatten() {
        env.rt.backend.print_str_trace(line);
    }
    env.rt.backend.print_str_trace("└");
    for _ in 0..max_line_len - 1 {
        env.rt.backend.print_str_trace("╴");
    }
    env.rt.backend.print_str_trace("\n");
    Ok(())
}

fn stack(env: &Uiua, inverse: bool) -> UiuaResult {
    let span = if inverse {
        format!("{}{} {}", Primitive::Un, Primitive::Stack, env.span())
    } else {
        format!("{} {}", Primitive::Stack, env.span())
    };
    let items = env.stack();
    let max_line_len = span.chars().count() + 2;
    let boundaries = stack_boundaries(env);
    let item_lines: Vec<Vec<String>> = items
        .iter()
        .map(Value::show)
        .map(|s| s.lines().map(Into::into).collect::<Vec<String>>())
        .map(|lines| format_trace_item_lines(lines, max_line_len))
        .enumerate()
        .flat_map(|(i, lines)| {
            if let Some((_, id)) = boundaries.iter().find(|(height, _)| i == *height) {
                let id = id.as_ref().map_or_else(String::new, ToString::to_string);
                vec![vec![format!("│╴╴╴{id}╶╶╶\n")], lines]
            } else {
                vec![lines]
            }
        })
        .collect();
    env.rt.backend.print_str_trace(&format!("┌╴{span}\n"));
    for line in item_lines.iter().flatten() {
        env.rt.backend.print_str_trace(line);
    }
    env.rt.backend.print_str_trace("└");
    for _ in 0..max_line_len - 1 {
        env.rt.backend.print_str_trace("╴");
    }
    env.rt.backend.print_str_trace("\n");
    Ok(())
}

fn dump(ops: Ops, env: &mut Uiua, inverse: bool) -> UiuaResult {
    let [f] = get_ops(ops, env)?;
    if f.sig != (1, 1) {
        return Err(env.error(format!(
            "{}'s function's signature must be |1, but it is {}",
            Primitive::Dump.format(),
            f.sig
        )));
    }
    let span = if inverse {
        format!("{}{} {}", Primitive::Un, Primitive::Dump, env.span())
    } else {
        format!("{} {}", Primitive::Dump, env.span())
    };
    let unprocessed = env.stack().to_vec();
    let mut items = Vec::new();
    for item in unprocessed {
        env.push(item);
        match env.exec(f.clone()) {
            Ok(()) => items.push(env.pop("dump's function's processed result")?),
            Err(e) => items.push(e.value()),
        }
    }
    let max_line_len = span.chars().count() + 2;
    let boundaries = stack_boundaries(env);
    let item_lines: Vec<Vec<String>> = items
        .iter()
        .map(Value::show)
        .map(|s| s.lines().map(Into::into).collect::<Vec<String>>())
        .map(|lines| format_trace_item_lines(lines, max_line_len))
        .enumerate()
        .flat_map(|(i, lines)| {
            if let Some((_, id)) = boundaries.iter().find(|(height, _)| i == *height) {
                let id = id.as_ref().map_or_else(String::new, ToString::to_string);
                vec![vec![format!("│╴╴╴{id}╶╶╶\n")], lines]
            } else {
                vec![lines]
            }
        })
        .collect();
    env.rt.backend.print_str_trace(&format!("┌╴{span}\n"));
    for line in item_lines.iter().flatten() {
        env.rt.backend.print_str_trace(line);
    }
    env.rt.backend.print_str_trace("└");
    for _ in 0..max_line_len - 1 {
        env.rt.backend.print_str_trace("╴");
    }
    env.rt.backend.print_str_trace("\n");
    Ok(())
}

fn stack_boundaries(env: &Uiua) -> Vec<(usize, &Option<FunctionId>)> {
    let mut boundaries: Vec<(usize, &Option<FunctionId>)> = Vec::new();
    let mut height = 0;
    for frame in env.call_frames().rev() {
        let delta = env.stack_height() as isize - frame.start_height as isize;
        height = height.max((frame.sig.args as isize + delta).max(0) as usize);
        if matches!(frame.id, Some(FunctionId::Main)) {
            break;
        }
        boundaries.push((env.stack_height().saturating_sub(height), &frame.id));
    }
    boundaries
}

fn format_trace_item_lines(mut lines: Vec<String>, mut max_line_len: usize) -> Vec<String> {
    let lines_len = lines.len();
    for (j, line) in lines.iter_mut().enumerate() {
        let stick = if lines_len == 1 || j == 1 {
            "├╴"
        } else {
            "│ "
        };
        line.insert_str(0, stick);
        max_line_len = max_line_len.max(line.chars().count());
        line.push('\n');
    }
    lines
}
