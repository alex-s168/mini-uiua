use std::{
    f64::consts::TAU,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use ecow::EcoVec;
use once_cell::sync::Lazy;

use crate::{
    Array, Boxed, SysBackend, Value, WILDCARD_NAN,
};

/// The definition of a shadowable constant
pub struct ConstantDef {
    /// The constant's name
    pub name: &'static str,
    /// The constant's class
    pub class: ConstClass,
    /// The constant's value
    pub value: Lazy<ConstantValue>,
}

/// The value of a shadowable constant
pub enum ConstantValue {
    /// A static value that is always the same
    Static(Value),
}

impl ConstantValue {
    /// Resolve the constant to a value
    pub(crate) fn resolve(
        &self,
        current_file_path: Option<&Path>,
        backend: &dyn SysBackend,
    ) -> Value {
        let current_file_path = current_file_path.map(|p| {
            let mut path = PathBuf::new();
            for comp in p.components() {
                path.push(comp);
            }
            path
        });
        match self {
            ConstantValue::Static(val) => val.clone(),
        }
    }
}

impl<T> From<T> for ConstantValue
where
    T: Into<Value>,
{
    fn from(val: T) -> Self {
        ConstantValue::Static(val.into())
    }
}

macro_rules! constant {
    ($($(#[doc = $doc:literal])+ ($(#[$attr:meta])* $name:literal, $class:ident, $value:expr)),* $(,)?) => {
        const COUNT: usize = {
            let mut count = 0;
            $(
                $(#[$attr])*
                {
                    _ = $name;
                    count += 1;
                }
            )*
            count
        };
        /// The list of all shadowable constants
        pub static CONSTANTS: [ConstantDef; COUNT] =
            [$(
                $(#[$attr])*
                ConstantDef {
                    name: $name,
                    value: Lazy::new(|| {$value.into()}),
                    class: ConstClass::$class,
                },
            )*];
    };
}

/// Kinds of shadowable constants
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstClass {
    Math,
    Color,
    Spatial,
}

constant!(
    /// Euler's constant
    ("e", Math, std::f64::consts::E),
    /// The imaginary unit
    ("i", Math, crate::Complex::I),
    /// IEEE 754-2008's `NaN`
    ("NaN", Math, f64::NAN),
    /// The wildcard `NaN` value that equals any other number
    ("W", Math, WILDCARD_NAN),
    /// The maximum integer that can be represented exactly
    ("MaxInt", Math, 2f64.powi(53)),
    /// The machine epsilon for Uiua numbers
    ///
    /// It is the difference between 1 and the next larger representable number.
    ("ε", Math, f64::EPSILON),
    /// 1-dimensional adjacent neighbors offsets
    ("A₁", Spatial, [1, -1]),
    /// 2-dimensional adjacent neighbors offsets
    ("A₂", Spatial, [[0, 1], [1, 0], [0, -1], [-1, 0]]),
    /// 3-dimensional adjacent neighbors offsets
    ("A₃", Spatial, [[0, 1, 0], [1, 0, 0], [0, -1, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]]),
    /// 2-dimensional corner neighbors offsets
    ("C₂", Spatial, [[1, 1], [1, -1], [-1, -1], [-1, 1]]),
    /// 3-dimensional corner neighbors offsets
    ("C₃", Spatial, [
        [1, 1, 1], [1, -1, 1], [-1, -1, 1], [-1, 1, 1],
        [1, 1, -1], [1, -1, -1], [-1, -1, -1], [-1, 1, -1]
    ]),
    /// 3-dimensional edge neighbors offsets
    ("E₃", Spatial, [
        [1, 1, 0], [1, -1, 0], [-1, -1, 0], [-1, 1, 0],
        [0, 1, 1], [1, 0, 1], [0, -1, 1], [-1, 0, 1],
        [0, 1, -1], [1, 0, -1], [0, -1, -1], [-1, 0, -1]
    ]),
    /// The hexadecimal digits
    ("HexDigits", Math, "0123456789abcdef"),
    /// The color white
    ("White", Color, [1.0, 1.0, 1.0]),
    /// The color black
    ("Black", Color, [0.0, 0.0, 0.0]),
    /// The color red
    ("Red", Color, [1.0, 0.0, 0.0]),
    /// The color orange
    ("Orange", Color, [1.0, 0.5, 0.0]),
    /// The color yellow
    ("Yellow", Color, [1.0, 1.0, 0.0]),
    /// The color green
    ("Green", Color, [0.0, 1.0, 0.0]),
    /// The color cyan
    ("Cyan", Color, [0.0, 1.0, 1.0]),
    /// The color blue
    ("Blue", Color, [0.0, 0.0, 1.0]),
    /// The color purple
    ("Purple", Color, [0.5, 0.0, 1.0]),
    /// The color magenta
    ("Magenta", Color, [1.0, 0.0, 1.0]),
);

