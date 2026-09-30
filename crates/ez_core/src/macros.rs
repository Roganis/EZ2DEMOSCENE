//! Small macros shared by the scene types.

/// A plain enum whose variants each carry a display label, with
/// `ALL` (every variant in declaration order, for pickers) and `label()`.
///
/// ```ignore
/// labeled_enum! {
///     #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
///     pub enum CameraMode {
///         /// Circles the target.
///         #[default]
///         Orbit => "Orbit",
///         Static => "Static",
///     }
/// }
/// ```
macro_rules! labeled_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident => $label:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant,
            )+
        }

        impl $name {
            pub const ALL: [$name; [$(stringify!($variant)),+].len()] = [$($name::$variant),+];

            pub fn label(self) -> &'static str {
                match self {
                    $($name::$variant => $label,)+
                }
            }
        }
    };
}
