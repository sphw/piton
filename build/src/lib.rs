mod rust;
mod ty;

use miette::{Context, Diagnostic, IntoDiagnostic, NamedSource, SourceSpan};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};
use thiserror::Error;
use ty::{LayoutChecker, TyChecker};

#[derive(Debug, Clone)]
pub enum Expr {
    Struct(Struct),
    Enum(Enum),
    Service(Service),
    Bus(Bus),
    Extern(Extern),
}

impl Expr {
    pub(crate) fn ty_def(&self) -> &TyDef {
        match self {
            Expr::Struct(s) => &s.ty_def,
            Expr::Enum(e) => &e.ty_def,
            Expr::Bus(b) => &b.ty_def,
            Expr::Service(s) => &s.ty_def,
            Expr::Extern(e) => &e.ty_def,
        }
    }

    pub(crate) fn field_tys(&mut self) -> Vec<(String, &mut Ty)> {
        match self {
            Expr::Struct(s) => s
                .fields
                .iter_mut()
                .map(|v| (v.name.clone(), &mut v.ty))
                .collect(),
            Expr::Enum(e) => e
                .variants
                .iter_mut()
                .flat_map(|v| Some((v.name.clone(), v.ty.as_mut()?)))
                .collect(),
            Expr::Service(s) => s
                .methods
                .iter_mut()
                .flat_map(|m| {
                    [
                        ("arg".to_string(), &mut m.arg_ty),
                        ("ret".to_string(), &mut m.return_ty),
                    ]
                })
                .collect(),
            Expr::Bus(b) => b
                .msgs
                .iter_mut()
                .map(|m| ("arg".to_string(), &mut m.ty))
                .collect(),
            Expr::Extern(_) => vec![],
        }
    }
}

peg::parser! {
    pub grammar piton_parser() for str {
        rule ty() -> Ty
            = "u8" { Ty::U8 }
            / "u16" { Ty::U16 }
            / "u32" { Ty::U32 }
            / "u64" { Ty::U64 }
            / "i8" { Ty::I8 }
            / "i16" { Ty::I16 }
            / "i32" { Ty::I32 }
            / "i64" { Ty::I64 }
            / "f32" { Ty::F32 }
            / "f64" { Ty::F64 }
            / "bool" { Ty::Bool }
            / "[" _ ty:ty() ";" _ len:uint() _ "]" { Ty::Array { ty: Box::new(ty), len } }
            / name:symbol() generic_args:generic_args()? {
                Ty::Unresolved {
                    name,
                    generic_args: generic_args.unwrap_or_default()
                }
            }

        rule symbol() -> String
            = s:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) { s.into() }

       rule uint() -> usize
            = n:$(['0'..='9']+) {? n.parse().map_err(|_| "number failed to parse") }

        rule struct_def() -> Struct
            = "struct" _ name:symbol() generic_tys:generic_tys()? _ "{" _ fields:(field() ** ("," _)) _ "}" {
                Struct {
                    ty_def: TyDef {
                        name,
                        generic_tys: generic_tys.unwrap_or_default()
                    },
                    fields,
                }
            }

        rule field() -> Field
            = name:symbol() ":" _ ty:ty() { Field { name , ty }}

        rule enum_def() -> Enum
            = "enum" _ name:symbol() generic_tys:generic_tys()? _ "{" _ variants:(variant() ** ("," _)) _ "}" {
                Enum {
                    ty_def: TyDef {
                        name,
                        generic_tys: generic_tys.unwrap_or_default()
                    },
                    variants,
                }
            }

        rule variant() -> Variant
            = name:symbol() ty:variant_ty()? { Variant { name, ty } }

        rule variant_ty() -> Ty
            = "(" ty:ty() ")" { ty }

        rule service_def() -> Service
            = "service" _ ty_def:ty_def() _ "{" _ methods:(method() ** ",") _ "}" {
                Service {
                    ty_def,
                    methods
                }
            }

        rule method() -> Method
            = "method" _ name:symbol() _ "(" arg_ty:ty() ")" _ "->" _ return_ty:ty() { Method { name, arg_ty, return_ty }}

        rule bus_def() -> Bus
            = "bus" _ ty_def:ty_def() _ "{" _ msgs:(msg() ** (";" _ )) _ "}" {
                Bus {
                    ty_def,
                    msgs
                }
            }

        rule ty_def() -> TyDef
            = name:symbol() generic_tys:generic_tys()? {
                TyDef {
                    name,
                    generic_tys: generic_tys.unwrap_or_default()
                }
            }

        rule msg() -> Msg
            = "msg" _ name:symbol() "(" ty:ty() ")"  { Msg { name, ty }}

        rule generic_ty() -> GenericTy
            = "const" _ name:symbol() ":" _  ty:ty() { GenericTy::Const { name, ty } }
            / name:symbol() { GenericTy::Ty(name) }

        rule generic_arg() -> GenericArg
            = ty:ty() { GenericArg::Ty(ty) }
            / uint:uint() { GenericArg::Const(uint) }

        rule generic_tys() -> Vec<GenericTy>
            = "<" _ args:generic_ty() ** ("," _) ">" { args }

        rule generic_args() -> Vec<GenericArg>
            = "<" _ args:generic_arg() ** ("," _) ">" { args }

        rule extern_def() -> Extern
            = "extern" _ ty_def:ty_def() _ "{" _ concrete_impls:(concrete() ** ",")   _  "}" {
                Extern {
                    ty_def,
                    concrete_impls: concrete_impls.into_iter().collect()
                }
            }

        rule concrete() -> (String, Vec<TemplateToken>)
            = "concrete" _ lang:symbol() _ "=" _ im:template() { (lang, im) }

        rule string() -> String
            = "\"" c:character()* "\"" { c.into_iter().collect() }

        rule template_symbol() -> String
            = "${" _ symbol:symbol() _  "}" { symbol }

        rule template_token() -> TemplateToken
            = t:template_symbol() { TemplateToken::Template(t) }
                /  !("${" / "}") c:character()  { TemplateToken::Char(c) }

        rule template() -> Vec<TemplateToken>
            = "t\"" t:template_token()* "\"" { t.into_iter().collect() }

        rule character() -> char
            = !("\"" / "\\" ) c:$([_]) {? c.chars().next().ok_or("char err") }
            / "\\u{" value:$(['0'..='9' | 'a'..='f' | 'A'..='F']+) "}" {? u32::from_str_radix(value, 16).map_err(|_| "invalid unicode point").and_then(|u| std::char::from_u32(u).ok_or("invalid char")) }
            / "\\x" value:$(['0'..='9' | 'a'..='f' | 'A'..='F']['0'..='9' | 'a'..='f' | 'A'..='F'])  {? u8::from_str_radix(value, 16).map_err(|_| "invalid u8").map(|u| u.into())}
            / "\\n" { '\n' } / "\\t" { '\t' } / "\\r" { '\r' }
            / expected!("valid escape sequence")

        rule expr() -> Expr
            = s:struct_def() { Expr::Struct(s) }
            / e:enum_def() { Expr::Enum(e) }
            / s:service_def() { Expr::Service(s) }
            / b:bus_def() { Expr::Bus(b) }
            / e:extern_def() { Expr::Extern(e) }


        rule _() = quiet!{[' ' | '\n' | '\t']*}

        pub rule exprs() -> Vec<Expr>
            = exprs:(expr:expr() ** _) _ { exprs }
    }
}

#[derive(Error, Debug, Diagnostic)]
#[error("syntax error")]
#[diagnostic()]
struct ParseError {
    #[source_code]
    src: NamedSource,
    #[label("{msg}")]
    source_span: SourceSpan,
    msg: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TemplateToken {
    Char(char),
    Template(String),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum GenericTy {
    Ty(String),
    Const { ty: Ty, name: String },
}

impl GenericTy {
    fn name(&self) -> &str {
        match self {
            GenericTy::Ty(name) => name,
            GenericTy::Const { name, .. } => name,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum GenericArg {
    Ty(Ty),
    Const(usize),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Extern {
    ty_def: TyDef,
    concrete_impls: HashMap<String, Vec<TemplateToken>>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TyDef {
    name: String,
    generic_tys: Vec<GenericTy>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Struct {
    ty_def: TyDef,
    fields: Vec<Field>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Field {
    name: String,
    ty: Ty,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Enum {
    ty_def: TyDef,
    variants: Vec<Variant>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Variant {
    name: String,
    ty: Option<Ty>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Service {
    pub ty_def: TyDef,
    pub methods: Vec<Method>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Method {
    pub name: String,
    pub arg_ty: Ty,
    pub return_ty: Ty,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Bus {
    pub ty_def: TyDef,
    pub msgs: Vec<Msg>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Msg {
    pub name: String,
    pub ty: Ty,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Ty {
    U64,
    U32,
    U16,
    U8,
    I64,
    I32,
    I16,
    I8,
    F32,
    F64,
    Bool,
    Array {
        ty: Box<Ty>,
        len: usize,
    },
    Unresolved {
        name: String,
        generic_args: Vec<GenericArg>,
    },
    Extern(String),
}

pub trait TypeGenerator {
    fn generate(&self, doc: &[Expr]) -> miette::Result<String> {
        let strings = doc
            .iter()
            .map(|e| match e {
                Expr::Struct(s) => self.generate_struct(s),
                Expr::Enum(e) => self.generate_enum(e),
                _ => Ok(String::default()),
            })
            .collect::<miette::Result<Vec<_>>>()?;
        Ok(strings.into_iter().fold(String::default(), |mut xs, x| {
            xs += &x;
            xs += "\n";
            xs
        }))
    }
    fn generate_struct(&self, s: &Struct) -> miette::Result<String>;
    fn generate_enum(&self, e: &Enum) -> miette::Result<String>;
}

pub trait ServiceGenerator {
    fn generate(&self, doc: &[Expr]) -> miette::Result<String> {
        let strings = doc
            .iter()
            .map(|e| {
                if let Expr::Service(s) = e {
                    self.generate_service(s)
                } else {
                    Ok(String::default())
                }
            })
            .collect::<miette::Result<Vec<_>>>()?;
        Ok(strings.into_iter().fold(String::default(), |mut xs, x| {
            xs += &x;
            xs += "\n";
            xs
        }))
    }
    fn generate_service(&self, service: &Service) -> miette::Result<String>;
}

pub trait BusGenerator {
    fn generate(&self, doc: &[Expr]) -> miette::Result<String> {
        let strings = doc
            .iter()
            .map(|e| {
                if let Expr::Bus(b) = e {
                    self.generate_bus(b)
                } else {
                    Ok(String::default())
                }
            })
            .collect::<miette::Result<Vec<_>>>()?;
        Ok(strings.into_iter().fold(String::default(), |mut xs, x| {
            xs += &x;
            xs += "\n";
            xs
        }))
    }
    fn generate_bus(&self, service: &Bus) -> miette::Result<String>;
}

#[derive(Default)]
pub struct RustBuilder {
    types: bool,
    server: bool,
    client: bool,
}

impl RustBuilder {
    pub fn types(mut self) -> Self {
        self.types = true;
        self
    }

    pub fn server(mut self) -> Self {
        self.server = true;
        self
    }

    pub fn client(mut self) -> Self {
        self.client = true;
        self
    }

    pub fn build(self, path: impl AsRef<Path>) -> miette::Result<()> {
        let path = path.as_ref();
        let doc = std::fs::read_to_string(path).into_diagnostic()?;
        let mut exprs = piton_parser::exprs(&doc).map_err(|err| ParseError {
            src: NamedSource::new(
                path.file_name()
                    .and_then(|s| s.to_str())
                    .expect("non utf8 filename"),
                doc,
            ),
            source_span: (err.location.offset, 1).into(),
            msg: format!("expected {}", err.expected),
        })?;

        let mut checker = TyChecker::default();
        for expr in &exprs {
            checker.visit_expr(expr);
        }
        for expr in &mut exprs {
            checker.resolve_expr(expr)?;
        }
        for expr in &mut exprs {
            let mut checker = LayoutChecker::default();
            if let Expr::Struct(ref mut s) = expr {
                let name = s.ty_def.name.clone();
                println!("struct {name}");
                for field in &s.fields {
                    checker
                        .next_field(field.ty.layout())
                        .wrap_err(format!("{name}, {}", field.name))?;
                }
                let final_pad = checker.final_pad();
                if final_pad > 0 {
                    s.fields.push(Field {
                        name: "_pad".to_string(),
                        ty: Ty::Extern(format!("piton::ZeroPad<{}>", final_pad)),
                    })
                }
            }
        }

        let mut o = String::default();
        if self.types {
            o += &rust::TypeGenerator.generate(&exprs)?;
        }
        if self.server {
            o += &rust::ServiceGenerator.generate(&exprs)?;
            o += &rust::BusRxGenerator.generate(&exprs)?;
        }
        if self.client {
            o += &rust::ClientGenerator.generate(&exprs)?;
            o += &rust::BusTxGenerator.generate(&exprs)?;
        }
        if self.client || self.server {
            o += &rust::ReqGenerator.generate(&exprs)?;
            o += &rust::MsgGenerator.generate(&exprs)?;
        }
        let out =
            &PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| miette::miette!("no out dir"))?);
        let file_stem = path
            .file_stem()
            .and_then(|f| f.to_str())
            .ok_or_else(|| miette::miette!("invalid file stem"))?;
        fs::write(out.join(format!("{}.rs", file_stem)), o).into_diagnostic()?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ZeroPad<const N: usize> {
    _pad: [u8; N],
}

impl<const N: usize> Default for ZeroPad<N> {
    fn default() -> Self {
        Self { _pad: [0; N] }
    }
}
