mod rust;

use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use knuffel::traits::ErrorSpan;
use miette::IntoDiagnostic;

pub enum Expr {
    Struct(Struct),
    Enum(Enum),
    Service(Service),
    Bus(Bus),
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
            / ty:symbol() { Ty::Unresolved(ty) }

        rule symbol() -> String
            = s:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) { s.into() }

       rule uint() -> usize
            = n:$(['0'..='9']+) {? n.parse().map_err(|_| "number failed to parse") }

        rule struct_def() -> Struct
            = "struct" _ name:symbol() _ "{" _ fields:(field() ** ("," _)) _ "}" { Struct { name, fields } }

        rule field() -> Field
            = name:symbol() ":" _ ty:ty() { Field { name , ty }}

        rule enum_def() -> Enum
            = "enum" _ name:symbol() _ "{" _ variants:(variant() ** ("," _)) _ "}" { Enum { name, variants }}

        rule variant() -> Variant
            = name:symbol() ty:variant_ty()? { Variant { name, ty } }

        rule variant_ty() -> Ty
            = "(" ty:ty() ")" { ty }

        rule service_def() -> Service
            = "service" _ name:symbol() _ "{" _ methods:(method() ** ",") _ "}" { Service { name, methods }}

        rule method() -> Method
            = "method" _ name:symbol() _ "(" arg_ty:ty() ")" _ "->" _ return_ty:ty() { Method { name, arg_ty, return_ty }}

        rule bus_def() -> Bus
            = "bus" _ name:symbol() _ "{" _ msgs:(msg() ** ",") _ "}" { Bus { name, msgs  }}

        rule msg() -> Msg
            = "msg" _ name:symbol() "(" ty:ty() ")"  { Msg { name, ty }}

        rule expr() -> Expr
            = s:struct_def() { Expr::Struct(s) }
            / e:enum_def() { Expr::Enum(e) }
            / s:service_def() { Expr::Service(s) }
            / b:bus_def() { Expr::Bus(b) }


        rule _() = quiet!{[' ' | '\n' | '\t']*}

        pub rule exprs() -> Vec<Expr>
            = exprs:(expr:expr() ** _) _ { exprs }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Struct {
    name: String,
    fields: Vec<Field>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Field {
    name: String,
    ty: Ty,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Enum {
    name: String,
    variants: Vec<Variant>,
}

#[derive(Debug, PartialEq, Eq)]
struct Variant {
    name: String,
    ty: Option<Ty>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub methods: Vec<Method>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Method {
    pub name: String,
    pub arg_ty: Ty,
    pub return_ty: Ty,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Bus {
    pub name: String,
    pub msgs: Vec<Msg>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Msg {
    pub name: String,
    pub ty: Ty,
}

#[derive(Debug, PartialEq, Eq)]
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
    Array { ty: Box<Ty>, len: usize },
    Unresolved(String),
}

impl<S: ErrorSpan> knuffel::DecodeScalar<S> for Ty {
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(typ) = type_name {
            ctx.emit_error(knuffel::errors::DecodeError::TypeName {
                span: typ.span().clone(),
                found: Some((**typ).clone()),
                expected: knuffel::errors::ExpectedType::no_type(),
                rust_type: "ty",
            });
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        _ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, knuffel::errors::DecodeError<S>> {
        match &**val {
            knuffel::ast::Literal::String(ref s) => {
                s.parse().map_err(
                    |err: &'static str| knuffel::errors::DecodeError::Unexpected {
                        span: val.span().clone(),
                        kind: "array",
                        message: err.to_string(),
                    },
                )
            }
            _ => Err(knuffel::errors::DecodeError::scalar_kind(
                knuffel::decode::Kind::String,
                val,
            )),
        }
    }
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


impl FromStr for Ty {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "u64" => Ty::U64,
            "u32" => Ty::U32,
            "u16" => Ty::U16,
            "u8" => Ty::U8,
            "i32" => Ty::I32,
            "i16" => Ty::I16,
            "i8" => Ty::I8,
            "bool" => Ty::Bool,
            s => {
                let mut chars = s.chars();
                if chars.next() == Some('[') {
                    if chars.last() == Some(']') {
                        let args = &s[1..s.len() - 1];
                        let split = args.split(';').collect::<Vec<_>>();
                        if split.len() != 2 {
                            return Err("array literal lacks the correct args");
                        }
                        let ty = split[0];
                        let count = split[1].trim();
                        let ty: Box<Ty> = Box::new(ty.parse()?);
                        let len: usize = count.parse().map_err(|_| "invalid count param")?;
                        Ty::Array { ty, len }
                    } else {
                        return Err("array literal was started but not finished");
                    }
                } else {
                    Ty::Unresolved(s.into())
                }
            }
        })
    }
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
        let exprs = piton_parser::exprs(&doc).into_diagnostic()?;
        let mut o = String::default();
        if self.types {
            o += &rust::TypeGenerator.generate(&exprs)?;
        }
        if self.server {
            o += &rust::ServiceGenerator.generate(&exprs)?;
        }
        if self.client {
            o += &rust::ClientGenerator.generate(&exprs)?;
        }
        if self.client || self.server {
            o += &rust::ReqGenerator.generate(&exprs)?;
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
