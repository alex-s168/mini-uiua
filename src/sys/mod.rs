use std::{
    any::Any,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use enum_iterator::{all, Sequence};

use crate::{
    algorithm::{multi_output, validate_size},
    get_ops,
    abort_txt,
    Array, Boxed, Ops, Primitive, Purity, Uiua, UiuaResult, Value,
};

macro_rules! sys_op {
    ($(
        #[doc = $doc_rust:literal]
        $(#[doc = $doc:literal])*
        (
            $args:literal$(($outputs:expr))?$([$mod_args:expr])?,
            $variant:ident, $class:ident, $name:literal, $long_name:literal
            $(,$purity:ident)*
        )
    ),* $(,)?) => {
        /// A system function
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Sequence)]
        pub enum SysOp {
            $(
                #[doc = $doc_rust]
                $variant
            ),*
        }

        impl SysOp {
            /// All system functions
            pub const ALL: [Self; 0 $(+ {stringify!($variant); 1})*] = [
                $(Self::$variant,)*
            ];
            /// Get the system function's short name
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant => $name),*
                }
            }
            /// Get the system function's long name
            pub fn long_name(&self) -> &'static str {
                match self {
                    $(Self::$variant => $long_name),*
                }
            }
            /// Get the number of arguments the system function expects
            pub fn args(&self) -> usize {
                match self {
                    $(SysOp::$variant => $args,)*
                }
            }
            /// Get the number of function arguments the system function expects if it is a modifier
            pub fn modifier_args(&self) -> Option<usize> {
                match self {
                    $($(
                        SysOp::$variant => Some($mod_args),
                    )?)*
                    _ => None
                }
            }
            /// Get the number of outputs the system function returns
            pub fn outputs(&self) -> usize {
                match self {
                    $($(SysOp::$variant => $outputs as usize,)?)*
                    _ => 1
                }
            }
            /// Get the system function's class
            pub fn class(&self) -> SysOpClass {
                match self {
                    $(SysOp::$variant => SysOpClass::$class),*
                }
            }
            /// Whether the system function is pure
            pub fn purity(&self) -> Purity {
                match self {
                    $($(SysOp::$variant => Purity::$purity,)*)*
                    _ => Purity::Impure
                }
            }
        }
    };
}

/// Categories of system functions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Sequence)]
#[allow(missing_docs)]
pub enum SysOpClass {
    Filesystem,
    StdIO,
    Env,
    Stream,
    Command,
    Media,
    Tcp,
    Ffi,
    Misc,
}

impl SysOpClass {
    /// All system function classes
    pub fn all() -> impl Iterator<Item = Self> {
        all()
    }
}

sys_op! {
    /// Exit the program with a status code
    (1(0), Exit, Misc, "&exit", "exit", Mutating),
    /// Sleep for n seconds
    ///
    /// On the web, this example will hang for 1 second.
    /// ex: ⚂ &sl 1
    (1(0), Sleep, Misc, "&sl", "sleep", Mutating),
    /// Read characters formed by at most n bytes from a stream
    ///
    /// Expects a count and a stream handle.
    /// The stream handle `0` is stdin.
    /// ex: &rs 4 &fo "example.txt"
    /// Using [infinity] as the count will read until the end of the stream.
    /// ex: &rs ∞ &fo "example.txt"
    ///
    /// [&rs] will attempt to read the given number of *bytes* from the stream.
    /// If the read bytes are not valid UTF-8, up to 3 additional bytes will be read in an attempt to finish a valid UTF-8 character.
    ///
    /// See also: [&rb]
    (2, ReadStr, Stream, "&rs", "read to string", Mutating),
    /// Read at most n bytes from a stream
    ///
    /// Expects a count and a stream handle.
    /// The stream handle `0` is stdin.
    /// ex: &rb 4 &fo "example.txt"
    /// Using [infinity] as the count will read until the end of the stream.
    /// ex: &rb ∞ &fo "example.txt"
    ///
    /// See also: [&rs]
    (2, ReadBytes, Stream, "&rb", "read to bytes", Mutating),
    /// Read from a stream until a delimiter is reached
    ///
    /// Expects a delimiter and a stream handle.
    /// The result will be a rank-`1` byte or character array. The type will match the type of the delimiter.
    /// The stream handle `0` is stdin.
    /// ex: &ru "Uiua" &fo "example.txt"
    (2, ReadUntil, Stream, "&ru", "read until", Mutating),
    /// Read lines from a stream
    ///
    /// [&rl] calls its function on each line in the stream without reading the entire stream into memory.
    /// Lines are delimited by either `\n` or `\r\n`.
    /// For each line, it will be pushed onto the stack and the function will be called.
    /// Additional arguments to the function will be bellow the line.
    /// Outputs in excess of the number of accumulators will be collected into arrays.
    (1[1], ReadLines, Stream, "&rl", "read lines", Mutating),
    /// Write an array to a stream
    ///
    /// If the stream is a file, the file may not be written to until it is closed with [&cl].
    /// The stream handle `1` is stdout.
    /// The stream handle `2` is stderr.
    /// ex: &cl &w "Hello, world!" . &fc "file.txt"
    ///   : &fras "file.txt"
    (2(0), Write, Stream, "&w", "write", Mutating),
    /// Close a stream by its handle
    ///
    /// This will close files, tcp listeners, and tcp sockets.
    (1(0), Close, Stream, "&cl", "close handle", Mutating),
    /// Open a file and return a handle to it
    ///
    /// ex: &fo "example.txt"
    /// The file can be read from with [&rs], [&rb], or [&ru].
    /// The file can be written to with [&w].
    /// In some cases, the file may not be actually written to until it is closed with [&cl].
    /// [under][&fo] calls [&cl] automatically.
    (1, FOpen, Filesystem, "&fo", "file - open"),
    /// Create a file and return a handle to it
    ///
    /// ex: &fc "file.txt"
    /// The file can be read from with [&rs], [&rb], or [&ru].
    /// The file can be written to with [&w].
    /// In some cases, the file may not be actually written to until it is closed with [&cl].
    /// [under][&fc] calls [&cl] automatically.
    (1, FCreate, Filesystem, "&fc", "file - create", Mutating),
    /// Create a directory
    ///
    /// ex: &fmd "path/to/dir"
    /// Nested directories will be created automatically.
    (1(0), FMakeDir, Filesystem, "&fmd", "file - make directory", Mutating),
    /// Delete a file or directory
    ///
    /// ex: &fde "example.txt"
    /// Deletes the file or directory at the given path.
    /// Be careful with this function, as deleted files and directories cannot be recovered!
    /// For a safer alternative, see [&ftr].
    (1(0), FDelete, Filesystem, "&fde", "file - delete", Mutating),
    /// Move a file or directory to the trash
    ///
    /// ex: &ftr "example.txt"
    /// Moves the file or directory at the given path to the trash.
    /// This is a safer alternative to [&fde].
    (1(0), FTrash, Filesystem, "&ftr", "file - trash", Mutating),
    /// Check if a file, directory, or symlink exists at a path
    ///
    /// ex: &fe "example.txt"
    /// ex: &fe "foo.bar"
    (1, FExists, Filesystem, "&fe", "file - exists"),
    /// List the contents of a directory
    ///
    /// The result is a list of boxed strings.
    /// ex: &fld "."
    (1, FListDir, Filesystem, "&fld", "file - list directory"),
    /// Check if a path is a file
    ///
    /// ex: &fif "example.txt"
    (1, FIsFile, Filesystem, "&fif", "file - is file"),
    /// Read all the contents of a file into a string
    ///
    /// Expects a path and returns a rank-`1` character array.
    ///
    /// ex: &fras "example.txt"
    /// You can use [under][&fras] to write back to the file after modifying the string.
    /// ex: ⍜&fras(⊂:"\n# Wow!") "example.txt"
    ///   : &p&fras "example.txt"
    ///
    /// See [&frab] for reading into a byte array.
    (1, FReadAllStr, Filesystem, "&fras", "file - read all to string"),
    /// Read all the contents of a file into a byte array
    ///
    /// Expects a path and returns a rank-`1` numeric array.
    ///
    /// ex: &frab "example.txt"
    /// You can use [under][&frab] to write back to the file after modifying the array.
    /// ex: ⍜&frab(⊂:-@\0"\n# Wow!") "example.txt"
    ///   : &p&fras "example.txt"
    ///
    /// See [&fras] for reading into a rank-`1` character array.
    (1, FReadAllBytes, Filesystem, "&frab", "file - read all to bytes"),
    /// Write the entire contents of an array to a file
    ///
    /// Expects a path and a rank-`1` array of either numbers or characters.
    /// The file will be created if it does not exist and overwritten if it does.
    ///
    /// The editor on the website has a virtual filesystem. Files written with [&fwa] can be read with [&fras] or [&frab].
    /// ex: Path ← "test.txt"
    ///   : &fwa Path +@A⇡26
    ///   : &fras Path
    (2(0), FWriteAll, Filesystem, "&fwa", "file - write all", Mutating),
}

/// A handle to an IO stream
///
/// 0 is stdin, 1 is stdout, 2 is stderr.
///
/// Other handles can be used by files or sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(pub u64);

impl Handle {
    const STDIN: Self = Self(0);
    const STDOUT: Self = Self(1);
    const STDERR: Self = Self(2);
    /// The first handle that can be used by the user
    pub const FIRST_UNRESERVED: Self = Self(3);
}

impl From<usize> for Handle {
    fn from(n: usize) -> Self {
        Self(n as u64)
    }
}

impl Handle {
    pub(crate) fn value(self, kind: HandleKind) -> Value {
        let mut arr = Array::from(self.0 as f64);
        arr.meta_mut().handle_kind = Some(kind);
        Boxed(arr.into()).into()
    }
}

impl Value {
    /// Attempt to convert the array to systme handle
    pub fn as_handle(&self, env: &Uiua, mut expected: &'static str) -> UiuaResult<Handle> {
        if expected.is_empty() {
            expected = "Expected value to be a stream handle";
        }
        match self {
            Value::Box(b) => {
                if let Some(b) = b.as_scalar() {
                    b.0.as_nat(env, expected).map(|h| Handle(h as u64))
                } else {
                    Err(env.error(format!("{expected}, but it is rank {}", b.rank())))
                }
            }
            value => value.as_nat(env, expected).map(|h| Handle(h as u64)),
        }
    }
}

/// The function type passed to `&rl`'s returned function
#[cfg(not(target_arch = "wasm32"))]
pub type ReadLinesFn<'a> = Box<dyn FnMut(String, &mut Uiua) -> UiuaResult + Send + 'a>;
/// The function type passed to `&rl`'s returned function
#[cfg(target_arch = "wasm32")]
pub type ReadLinesFn<'a> = Box<dyn FnMut(String, &mut Uiua) -> UiuaResult + 'a>;

/// The function type returned by `&rl`
#[cfg(not(target_arch = "wasm32"))]
pub type ReadLinesReturnFn<'a> = Box<dyn FnMut(&mut Uiua, ReadLinesFn) -> UiuaResult + Send + 'a>;
/// The function type returned by `&rl`
#[cfg(target_arch = "wasm32")]
pub type ReadLinesReturnFn<'a> = Box<dyn FnMut(&mut Uiua, ReadLinesFn) -> UiuaResult + 'a>;

/// The function type passed to `&ast`
#[cfg(not(target_arch = "wasm32"))]
pub type AudioStreamFn = Box<dyn FnMut(&[f64]) -> UiuaResult<Vec<[f64; 2]>> + Send>;
/// The function type passed to `&ast`
#[cfg(target_arch = "wasm32")]
pub type AudioStreamFn = Box<dyn FnMut(&[f64]) -> UiuaResult<Vec<[f64; 2]>>>;

/// The kind of a handle
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum HandleKind {
    File(PathBuf),
    ChildStdin(String),
    ChildStdout(String),
    ChildStderr(String),
}

impl fmt::Display for HandleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(f, "file {}", path.display()),
            Self::ChildStdin(com) => write!(f, "stdin {com}"),
            Self::ChildStdout(com) => write!(f, "stdout {com}"),
            Self::ChildStderr(com) => write!(f, "stderr {com}"),
        }
    }
}

/// Trait for defining a system backend
#[allow(unused_variables)]
pub trait SysBackend: Any + Send + Sync + 'static {
    /// Cast the backend to `&dyn Any`
    fn any(&self) -> &dyn Any;
    /// Cast the backend to `&mut dyn Any`
    fn any_mut(&mut self) -> &mut dyn Any;
    /// Save a color-formatted version of an error message for later printing
    fn save_error_color(&self, message: String, colored: String) {}
    /// Check whether output is enabled
    fn output_enabled(&self) -> bool {
        true
    }
    /// Set whether output should be enabled
    ///
    /// Returns the previous value.
    ///
    /// It is the trait implementor's responsibility to ensure that this value is respected.
    fn set_output_enabled(&self, enabled: bool) -> bool {
        true
    }
    /// Print a string (without a newline) to stdout
    fn print_str_stdout(&self, s: &str) -> Result<(), String> {
        Err("Printing to stdout is not supported in this environment".into())
    }
    /// Print a string (without a newline) to stderr
    fn print_str_stderr(&self, s: &str) -> Result<(), String> {
        Err("Printing to stderr is not supported in this environment".into())
    }
    /// Print a string that was create by `trace`
    fn print_str_trace(&self, s: &str) {}
    /// Exit the program with a status code
    fn exit(&self, status: i32) -> Result<(), String> {
        Err("Exiting is not supported in this environment".into())
    }
    /// Check if a file or directory exists
    fn file_exists(&self, path: &str) -> bool {
        false
    }
    /// List the contents of a directory
    fn list_dir(&self, path: &str) -> Result<Vec<String>, String> {
        abort_txt("files not sup");
    }
    /// Check if a path is a file
    fn is_file(&self, path: &str) -> Result<bool, String> {
        abort_txt("files not sup");
    }
    /// Delete a file or directory
    fn delete(&self, path: &str) -> Result<(), String> {
        abort_txt("files not sup");
    }
    /// Move a file or directory to the trash
    fn trash(&self, path: &str) -> Result<(), String> {
        abort_txt("files not sup");
    }
    /// Read at most `count` bytes from a stream
    fn read(&self, handle: Handle, count: usize) -> Result<Vec<u8>, String> {
        abort_txt("files not sup");
    }
    /// Read from a stream until the end
    fn read_all(&self, handle: Handle) -> Result<Vec<u8>, String> {
        abort_txt("files not sup");
    }
    /// Read from a stream until a delimiter is reached
    fn read_until(&self, handle: Handle, delim: &[u8]) -> Result<Vec<u8>, String> {
        let mut buffer = Vec::new();
        loop {
            let bytes = self.read(handle, 1)?;
            if bytes.is_empty() {
                break;
            }
            buffer.extend_from_slice(&bytes);
            if buffer.ends_with(delim) {
                break;
            }
        }
        Ok(buffer)
    }
    /// Read lines from a stream
    fn read_lines<'a>(&self, handle: Handle) -> Result<ReadLinesReturnFn<'a>, String> {
        abort_txt("files not sup");
    }
    /// Write bytes to a stream
    fn write(&self, handle: Handle, contents: &[u8]) -> Result<(), String> {
        abort_txt("files not sup");
    }
    /// Create a file
    fn create_file(&self, path: &Path) -> Result<Handle, String> {
        abort_txt("files not sup");
    }
    /// Open a file
    fn open_file(&self, path: &Path, write: bool) -> Result<Handle, String> {
        abort_txt("files not sup");
    }
    /// Create a directory
    fn make_dir(&self, path: &Path) -> Result<(), String> {
        abort_txt("mkdir not sup");
    }
    /// Read all bytes from a file
    fn file_read_all(&self, path: &Path) -> Result<Vec<u8>, String> {
        let handle = self.open_file(path, false)?;
        let bytes = self.read(handle, usize::MAX)?;
        self.close(handle)?;
        Ok(bytes)
    }
    /// Write all bytes to a file
    fn file_write_all(&self, path: &Path, contents: &[u8]) -> Result<(), String> {
        let handle = self.create_file(path)?;
        self.write(handle, contents)?;
        self.close(handle)?;
        Ok(())
    }
    /// Sleep the current thread for `seconds` seconds
    fn sleep(&self, seconds: f64) -> Result<(), String> {
        abort_txt("sleep not sup");
    }
    /// The result of the `now` function
    ///
    /// Should be in seconds
    fn now(&self) -> f64 {
        now()
    }
    /// Close a stream
    fn close(&self, handle: Handle) -> Result<(), String> {
        Ok(())
    }
    /// Load a git repo as a module
    ///
    /// The returned path should be loadable via [`SysBackend::file_read_all`]
    fn load_git_module(&self, url: &str, target: GitTarget) -> Result<PathBuf, String> {
        abort_txt("load git mod not sup");
    }
    /// Get the local timezone offset in hours
    fn timezone(&self) -> Result<f64, String> {
        abort_txt("timezone not sup");
    }
}

/// A target for a git repository
#[derive(Debug, Clone, Default)]
pub enum GitTarget {
    /// The latest commit on the default branch
    #[default]
    Default,
    /// The latest commit on a specific branch
    Branch(String),
    /// A specific commit
    Commit(String),
}

impl fmt::Debug for dyn SysBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<sys backend>")
    }
}

/// A safe backend with no IO other than captured stdout and stderr
#[derive(Default)]
pub struct SafeSys {}

impl SysBackend for SafeSys {
    fn any(&self) -> &dyn Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn print_str_stdout(&self, s: &str) -> Result<(), String> {
        Ok(())
    }
    fn print_str_stderr(&self, s: &str) -> Result<(), String> {
        Ok(())
    }
}

impl SafeSys {
    /// Create a new safe system backend
    pub fn new() -> Self {
        Self::default()
    }
}

/// Trait for converting to a system backend
pub trait IntoSysBackend {
    /// Convert to a reference counted system backend
    fn into_sys_backend(self) -> Arc<dyn SysBackend>;
}

impl<T> IntoSysBackend for T
where
    T: SysBackend + Send + Sync + 'static,
{
    fn into_sys_backend(self) -> Arc<dyn SysBackend> {
        Arc::new(self)
    }
}

impl IntoSysBackend for Arc<dyn SysBackend> {
    fn into_sys_backend(self) -> Arc<dyn SysBackend> {
        self
    }
}

impl SysOp {
    pub(crate) fn run(&self, env: &mut Uiua) -> UiuaResult {
        match self {
            SysOp::Exit => {
                let status = env.pop(1)?.as_int(env, "Status must be an integer")? as i32;
                (env.rt.backend).exit(status).map_err(|e| env.error(e))?;
            }
            SysOp::FOpen => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let handle = (env.rt.backend)
                    .open_file(path.as_ref(), true)
                    .map_err(|e| env.error(e))?
                    .value(HandleKind::File(path.into()));
                env.push(handle);
            }
            SysOp::FCreate => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let handle: Value = (env.rt.backend)
                    .create_file(path.as_ref())
                    .map_err(|e| env.error(e))?
                    .value(HandleKind::File(path.into()));
                env.push(handle);
            }
            SysOp::FMakeDir => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                (env.rt.backend)
                    .make_dir(path.as_ref())
                    .map_err(|e| env.error(e))?;
            }
            SysOp::FDelete => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                env.rt.backend.delete(&path).map_err(|e| env.error(e))?;
            }
            SysOp::FTrash => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                env.rt.backend.trash(&path).map_err(|e| env.error(e))?;
            }
            SysOp::ReadStr => {
                let count = env
                    .pop(1)?
                    .as_nat_or_inf(env, "Count must be an integer or infinity")?;
                if let Some(count) = count {
                    validate_size::<char>([count], env)?;
                }
                let handle = env.pop(2)?.as_handle(env, "")?;
                let s = match handle {
                    Handle::STDOUT => abort_txt("stdstr no read"),
                    Handle::STDERR => abort_txt("stdstr no read"),
                    Handle::STDIN => abort_txt("stdstr no read"),
                    _ => {
                        if let Some(count) = count {
                            let buf = env
                                .rt
                                .backend
                                .read(handle, count)
                                .map_err(|e| env.error(e))?;
                            match String::from_utf8(buf) {
                                Ok(s) => s,
                                Err(e) => {
                                    let valid_to = e.utf8_error().valid_up_to();
                                    let mut buf = e.into_bytes();
                                    let mut rest = buf.split_off(valid_to);
                                    for _ in 0..3 {
                                        rest.extend(
                                            env.rt
                                                .backend
                                                .read(handle, 1)
                                                .map_err(|e| env.error(e))?,
                                        );
                                        if let Ok(s) = std::str::from_utf8(&rest) {
                                            buf.extend_from_slice(s.as_bytes());
                                            break;
                                        }
                                    }
                                    String::from_utf8(buf).map_err(|e| env.error(e))?
                                }
                            }
                        } else {
                            let bytes =
                                env.rt.backend.read_all(handle).map_err(|e| env.error(e))?;
                            String::from_utf8(bytes).map_err(|e| env.error(e))?
                        }
                    }
                };
                env.push(s);
            }
            SysOp::ReadBytes => {
                let count = env
                    .pop(1)?
                    .as_nat_or_inf(env, "Count must be an integer or infinity")?;
                if let Some(count) = count {
                    validate_size::<u8>([count], env)?;
                }
                let handle = env.pop(2)?.as_handle(env, "")?;
                let bytes = match handle {
                    Handle::STDOUT |
                    Handle::STDERR |
                    Handle::STDIN => abort_txt("stdstr no read"),
                    _ => {
                        if let Some(count) = count {
                            env.rt
                                .backend
                                .read(handle, count)
                                .map_err(|e| env.error(e))?
                        } else {
                            env.rt.backend.read_all(handle).map_err(|e| env.error(e))?
                        }
                    }
                };
                env.push(Array::from(bytes.as_slice()));
            }
            SysOp::ReadUntil => {
                let delim = env.pop(1)?;
                let handle = env.pop(2)?.as_handle(env, "")?;
                if delim.rank() > 1 {
                    return Err(env.error("Delimiter must be a rank 0 or 1 string or byte array"));
                }
                match handle {
                    Handle::STDOUT => abort_txt("read from str is no ok"),
                    Handle::STDERR => abort_txt("read from str is no ok"),
                    Handle::STDIN => abort_txt("read from str is no ok"),
                    _ => match delim {
                        Value::Num(arr) => {
                            let delim: Vec<u8> = arr.data.iter().map(|&x| x as u8).collect();
                            let bytes = env
                                .rt
                                .backend
                                .read_until(handle, &delim)
                                .map_err(|e| env.error(e))?;
                            env.push(Array::from(bytes.as_slice()));
                        }
                        Value::Byte(arr) => {
                            let delim: Vec<u8> = arr.data.into();
                            let bytes = env
                                .rt
                                .backend
                                .read_until(handle, &delim)
                                .map_err(|e| env.error(e))?;
                            env.push(Array::from(bytes.as_slice()));
                        }
                        Value::Char(arr) => {
                            let delim: Vec<u8> = arr.data.iter().collect::<String>().into();
                            let bytes = env
                                .rt
                                .backend
                                .read_until(handle, &delim)
                                .map_err(|e| env.error(e))?;
                            let s = String::from_utf8(bytes).map_err(|e| env.error(e))?;
                            env.push(s);
                        }
                        _ => return Err(env.error("Delimiter must be a string or byte array")),
                    },
                }
            }
            SysOp::Write => {
                let data = env.pop(1)?;
                let handle = env.pop(2)?.as_handle(env, "")?;
                let bytes: Vec<u8> = match data {
                    Value::Num(arr) => arr.data.iter().map(|&x| x as u8).collect(),
                    Value::Byte(arr) => arr.data.into(),
                    Value::Char(arr) => arr.data.iter().collect::<String>().into(),
                    Value::Box(_) => return Err(env.error("Cannot write box array")),
                };
                match handle {
                    Handle::STDOUT => env
                        .rt
                        .backend
                        .print_str_stdout(&String::from_utf8_lossy(&bytes))
                        .map_err(|e| env.error(e))?,
                    Handle::STDERR => env
                        .rt
                        .backend
                        .print_str_stderr(&String::from_utf8_lossy(&bytes))
                        .map_err(|e| env.error(e))?,
                    Handle::STDIN => return Err(env.error("Cannot write to stdin")),
                    _ => env
                        .rt
                        .backend
                        .write(handle, &bytes)
                        .map_err(|e| env.error(e))?,
                }
            }
            SysOp::FReadAllStr => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let bytes = (env.rt.backend)
                    .file_read_all(path.as_ref())
                    .map_err(|e| env.error(e))?;
                let s = String::from_utf8(bytes).map_err(|e| env.error(e))?;
                env.push(s);
            }
            SysOp::FReadAllBytes => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let bytes = (env.rt.backend)
                    .file_read_all(path.as_ref())
                    .map_err(|e| env.error(e))?;
                let bytes = bytes.into_iter().map(Into::into);
                env.push(Array::<u8>::from_iter(bytes));
            }
            SysOp::FWriteAll => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let data = env.pop(2)?;
                let bytes: Vec<u8> = match data {
                    Value::Num(arr) => arr.data.iter().map(|&x| x as u8).collect(),
                    Value::Byte(arr) => arr.data.into(),

                    Value::Char(arr) => arr.data.iter().collect::<String>().into(),
                    Value::Box(_) => return Err(env.error("Cannot write box array to file")),
                };
                (env.rt.backend)
                    .file_write_all(path.as_ref(), &bytes)
                    .map_err(|e| env.error(e))?;
            }
            SysOp::FExists => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let exists = env.rt.backend.file_exists(&path);
                env.push(exists);
            }
            SysOp::FListDir => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let paths = env.rt.backend.list_dir(&path).map_err(|e| env.error(e))?;
                env.push(Array::<Boxed>::from_iter(paths));
            }
            SysOp::FIsFile => {
                let path = env.pop(1)?.as_string(env, "Path must be a string")?;
                let is_file = env.rt.backend.is_file(&path).map_err(|e| env.error(e))?;
                env.push(is_file);
            }
            SysOp::Sleep => {
                let mut seconds = env.pop(1)?.as_num(env, "Sleep time must be a number")?;
                if seconds < 0.0 {
                    return Err(env.error("Sleep time must be positive"));
                }
                if seconds.is_infinite() {
                    return Err(env.error("Sleep time cannot be infinite"));
                }
                if let Some(limit) = env.rt.execution_limit {
                    let elapsed = env.rt.backend.now() - env.rt.execution_start;
                    let max = limit - elapsed;
                    seconds = seconds.min(max);
                }
                env.rt.backend.sleep(seconds).map_err(|e| env.error(e))?;
            }
            SysOp::Close => {
                let handle = env.pop(1)?.as_handle(env, "")?;
                env.rt.backend.close(handle).map_err(|e| env.error(e))?;
            }
            prim => {
                return Err(env.error(if prim.modifier_args().is_some() {
                    format!(
                        "{} was not handled as a modifier. \
                        This is a bug in the interpreter",
                        Primitive::Sys(*prim)
                    )
                } else {
                    format!(
                        "{} was not handled as a function. \
                        This is a bug in the interpreter",
                        Primitive::Sys(*prim)
                    )
                }))
            }
        }
        Ok(())
    }
    pub(crate) fn run_mod(&self, ops: Ops, env: &mut Uiua) -> UiuaResult {
        match self {
            SysOp::ReadLines => {
                let [f] = get_ops(ops, env)?;
                let handle = env.pop(1)?.as_handle(env, "")?;
                let mut read_lines = env
                    .rt
                    .backend
                    .read_lines(handle)
                    .map_err(|e| env.error(e))?;
                let sig = f.sig;
                if sig.args == 0 {
                    return env.exec(f);
                }
                let acc_count = sig.args.saturating_sub(1);
                let out_count = sig.outputs.saturating_sub(acc_count);
                let mut outputs = multi_output(out_count, Vec::new());
                env.without_fill(|env| {
                    read_lines(
                        env,
                        Box::new(|s, env| {
                            let val = Value::from(s);
                            env.push(val);
                            env.exec(f.clone())?;
                            for i in 0..out_count {
                                outputs[i].push(env.pop("read lines output")?);
                            }
                            Ok(())
                        }),
                    )
                })?;
                for rows in outputs.into_iter().rev() {
                    let val = Value::from_row_values(rows, env)?;
                    env.push(val);
                }
            }
            prim => {
                return Err(env.error(if prim.modifier_args().is_some() {
                    format!(
                        "{} was not handled as a modifier. \
                        This is a bug in the interpreter",
                        Primitive::Sys(*prim)
                    )
                } else {
                    format!(
                        "{} was handled as a modifier. \
                        This is a bug in the interpreter",
                        Primitive::Sys(*prim)
                    )
                }))
            }
        }
        Ok(())
    }
}

/// Get the current time in seconds
///
/// This function works on both native and web targets.
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

pub(crate) fn terminal_size() -> Option<(usize, usize)> {
    None
}
