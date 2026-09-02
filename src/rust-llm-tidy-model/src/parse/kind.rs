//! The item-kind classification shared by every language backend.
//!
//! [`ItemKind`] names what a parsed item is; the Rust parse produces the
//! Rust variants, and C-family backends produce the C-family variants in
//! addition to the kind-agnostic ones (`Fn`, `Enum`, `Const`, ...).

use std::fmt;

/// Kind of a parsed source item.
///
/// Each variant is shown with the minimal syntax that produces it: Rust
/// syntax for the Rust variants, C# syntax for the C-family variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A free function (a method, at member level).
    ///
    /// ```rust,ignore
    /// fn main() {}
    /// ```
    Fn,
    /// A struct definition.
    ///
    /// ```rust,ignore
    /// struct Foo { x: i32 }
    /// ```
    Struct,
    /// An enum definition.
    ///
    /// ```rust,ignore
    /// enum Color { Red, Green, Blue }
    /// ```
    Enum,
    /// A type alias.
    ///
    /// ```rust,ignore
    /// type Point = (i32, i32);
    /// ```
    Type,
    /// A union definition.
    ///
    /// ```rust,ignore
    /// union Bytes { as_u32: u32, as_bytes: [u8; 4] }
    /// ```
    Union,
    /// An inherent or trait impl block.
    ///
    /// ```rust,ignore
    /// impl Foo {}
    /// impl Default for Foo { fn default() -> Self { Self {} } }
    /// ```
    Impl,
    /// A `use` import.
    ///
    /// ```rust,ignore
    /// use std::io;
    /// ```
    Use,
    /// A `const` item or constant field.
    ///
    /// ```rust,ignore
    /// const MAX: u32 = 100;
    /// ```
    Const,
    /// A `static` item.
    ///
    /// ```rust,ignore
    /// static COUNTER: AtomicUsize = AtomicUsize::new(0);
    /// ```
    Static,
    /// A module declaration or inline module.
    ///
    /// ```rust,ignore
    /// mod foo;
    /// mod bar {}
    /// ```
    Mod,
    /// An `extern crate` declaration.
    ///
    /// ```rust,ignore
    /// extern crate serde;
    /// ```
    Extern,
    /// A trait definition.
    ///
    /// ```rust,ignore
    /// trait Draw { fn render(&self); }
    /// ```
    Trait,
    /// A `macro_rules!` definition.
    ///
    /// ```rust,ignore
    /// macro_rules! say_hello { () => { println!("hi"); }; }
    /// ```
    Macro,
    /// A top-level macro invocation (e.g. `foo!();`) that is not a
    /// `macro_rules!` definition. Named after the last path segment so the
    /// graph stage can pair it with its local `macro_rules!` definition.
    ///
    /// ```rust,ignore
    /// println!("x");
    /// ```
    MacroInvocation,
    /// A namespace declaration (C-family).
    ///
    /// ```csharp
    /// namespace App.Models;
    /// ```
    Namespace,
    /// A class declaration (C-family).
    ///
    /// ```csharp
    /// class Service { }
    /// ```
    Class,
    /// An interface declaration (C-family).
    ///
    /// ```csharp
    /// interface IRepository { }
    /// ```
    Interface,
    /// A using directive (C-family).
    ///
    /// ```csharp
    /// using System.IO;
    /// ```
    Using,
    /// A property or indexer declaration (C-family).
    ///
    /// ```csharp
    /// int Count { get; set; }
    /// ```
    Property,
    /// An event declaration (C-family).
    ///
    /// ```csharp
    /// event EventHandler Changed;
    /// ```
    Event,
    /// A constructor declaration (C-family).
    ///
    /// ```csharp
    /// Service(int count) { }
    /// ```
    Constructor,
    /// A finalizer declaration (C-family).
    ///
    /// ```csharp
    /// ~Service() { }
    /// ```
    Destructor,
    /// Any other top-level item not covered above (foreign modules, trait
    /// aliases, verbatim items).
    ///
    /// ```rust,ignore
    /// extern "C" { fn f(); }
    /// ```
    Other,
}

impl ItemKind {
    /// The stable `&'static str` form of this kind (e.g. `"fn"`), shared by
    /// [`Display`] and structured change/diagnostic reporting so
    /// records can hold the kind without an owned allocation.
    ///
    /// [`Display`]: std::fmt::Display
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Type => "type",
            ItemKind::Union => "union",
            ItemKind::Impl => "impl",
            ItemKind::Use => "use",
            ItemKind::Const => "const",
            ItemKind::Static => "static",
            ItemKind::Mod => "mod",
            ItemKind::Extern => "extern",
            ItemKind::Trait => "trait",
            ItemKind::Macro => "macro",
            ItemKind::MacroInvocation => "macro-invocation",
            ItemKind::Namespace => "namespace",
            ItemKind::Class => "class",
            ItemKind::Interface => "interface",
            ItemKind::Using => "using",
            ItemKind::Property => "property",
            ItemKind::Event => "event",
            ItemKind::Constructor => "constructor",
            ItemKind::Destructor => "destructor",
            ItemKind::Other => "other",
        }
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
