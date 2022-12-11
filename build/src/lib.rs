mod rust;

use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use knuffel::traits::ErrorSpan;
use miette::{miette, IntoDiagnostic};
use smol_str::SmolStr;
#[derive(Debug, knuffel::Decode, PartialEq, Eq)]
pub struct Document {
    #[knuffel(children(name = "struct"))]
    structs: Vec<Struct>,
    #[knuffel(children(name = "enum"))]
    enums: Vec<Enum>,
    #[knuffel(children(name = "service"))]
    services: Vec<Service>,
}

impl Document {
    fn validate(&self) -> Result<(), miette::Report> {
        let mut ty_set: HashSet<String> = HashSet::default();
        for s in &self.structs {
            ty_set.insert(s.name.clone());
        }
        for e in &self.enums {
            ty_set.insert(e.name.clone());
            let mut variants: HashSet<String> = HashSet::default();
            for variant in &e.variants {
                if variants.contains(&variant.name) {
                    return Err(miette!("duplicate variant name {}", variant.name));
                }
                variants.insert(variant.name.clone());
            }
        }
        let unresolved_tys = self
            .structs
            .iter()
            .flat_map(|s| &s.fields)
            .map(|s| &s.ty)
            .chain(
                self.enums
                    .iter()
                    .flat_map(|e| &e.variants)
                    .filter_map(|f| f.ty.as_ref()),
            )
            .chain(
                self.services
                    .iter()
                    .flat_map(|s| &s.methods)
                    .filter_map(|m| m.arg_ty.as_ref()),
            )
            .chain(
                self.services
                    .iter()
                    .flat_map(|s| &s.methods)
                    .filter_map(|m| m.return_ty.as_ref()),
            )
            .filter_map(|ty| match &ty {
                Ty::Array { ty, .. } => match ty.as_ref() {
                    Ty::Unresolved(ty_str) => Some(ty_str.clone()),
                    _ => None,
                },
                Ty::Unresolved(ty) => Some(ty.clone()),
                _ => None,
            });
        for ty in unresolved_tys {
            if !ty_set.contains(&ty.to_string()) {
                return Err(miette!("unknown type {}", ty));
            }
        }
        Ok(())
    }
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
pub struct Struct {
    #[knuffel(argument)]
    name: String,
    #[knuffel(children(name = "field"))]
    fields: Vec<Field>,
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
pub struct Field {
    #[knuffel(argument)]
    name: String,
    #[knuffel(argument)]
    ty: Ty,
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
pub struct Enum {
    #[knuffel(argument)]
    name: String,
    #[knuffel(children(name = "variant"))]
    variants: Vec<Variant>,
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
struct Variant {
    #[knuffel(argument)]
    name: String,
    #[knuffel(argument)]
    ty: Option<Ty>,
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
pub struct Service {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(children(name = "method"))]
    pub methods: Vec<Method>,
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
pub struct Method {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(argument)]
    pub arg_ty: Option<Ty>,
    #[knuffel(argument)]
    pub return_ty: Option<Ty>,
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
    Bool,
    Array { ty: Box<Ty>, len: usize },
    Unresolved(SmolStr),
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
    fn generate(&self, doc: &Document) -> miette::Result<String> {
        let strings = doc
            .structs
            .iter()
            .map(|s| self.generate_struct(s))
            .chain(doc.enums.iter().map(|e| self.generate_enum(e)))
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
    fn generate(&self, doc: &Document) -> miette::Result<String> {
        let strings = doc
            .services
            .iter()
            .map(|s| self.generate_service(s))
            .collect::<miette::Result<Vec<_>>>()?;
        Ok(strings.into_iter().fold(String::default(), |mut xs, x| {
            xs += &x;
            xs += "\n";
            xs
        }))
    }
    fn generate_service(&self, service: &Service) -> miette::Result<String>;
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
        let doc: Document = knuffel::parse(
            path.file_name()
                .and_then(|f| f.to_str())
                .ok_or_else(|| miette::miette!("invalid file name"))?,
            &doc,
        )?;
        doc.validate()?;
        let mut o = String::default();
        if self.types {
            o += &rust::TypeGenerator.generate(&doc)?;
        }
        if self.server {
            o += &rust::ServiceGenerator.generate(&doc)?;
        }
        if self.client {
            o += &rust::ClientGenerator.generate(&doc)?;
        }
        if self.client || self.server {
            o += &rust::ReqGenerator.generate(&doc)?;
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

// pub fn build_server(path: impl AsRef<Path>) -> miette::Result<()> {
// }

// pub fn build_client(path: impl AsRef<Path>) -> miette::Result<()> {
//     let path = path.as_ref();
//     let doc = std::fs::read_to_string(path).into_diagnostic()?;
//     let doc: Document = knuffel::parse(
//         path.file_name()
//             .and_then(|f| f.to_str())
//             .ok_or_else(|| miette::miette!("invalid file name"))?,
//         &doc,
//     )?;
//     doc.validate()?;
//     let types = rust::TypeGenerator.generate(&doc)?;
//     let services = rust::ClientGenerator.generate(&doc)?;
//     let mut doc = types;
//     doc += "\n";
//     doc += &services;
//     let out = &PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| miette::miette!("no out dir"))?);
//     let file_stem = path
//         .file_stem()
//         .and_then(|f| f.to_str())
//         .ok_or_else(|| miette::miette!("invalid file stem"))?;
//     fs::write(out.join(format!("{}-client.rs", file_stem)), doc).into_diagnostic()?;
//     Ok(())
// }

#[cfg(test)]
mod tests {
    use crate::{Field, Struct, Ty};

    #[test]
    fn test_decode() {
        let doc = r#"
struct "Test" {
 field "foo" "u16"
 field "bar" "u32"
 field "boolean" "bool"
 field "array" "[u8; 20]"
}
    "#;
        let doc: crate::Document = knuffel::parse("test.kdl", doc).unwrap();
        assert_eq!(
            doc.structs[0],
            Struct {
                name: "Test".to_string(),
                fields: vec![
                    Field {
                        name: "foo".to_string(),
                        ty: Ty::U16
                    },
                    Field {
                        name: "bar".to_string(),
                        ty: Ty::U32
                    },
                    Field {
                        name: "boolean".to_string(),
                        ty: Ty::Bool
                    },
                    Field {
                        name: "array".to_string(),
                        ty: Ty::Array {
                            ty: Box::new(Ty::U8),
                            len: 20
                        }
                    },
                ]
            }
        )
    }

    #[test]
    fn test_validate() {
        let doc = r#"
struct "Test" {
 field "foo" "u16"
 field "bar" "u32"
 field "boolean" "bool"
 field "array" "[u8; 20]"
}
struct "Foo" {
 field "foo" "Foo"
 field "bar" "Bar"
}
enum "Bar" {
  variant "test"
  variant "b" "u8"
}
service "Driver" {
   method "send" "Bar" "Foo"
   method "close"
}
"#;
        let doc: crate::Document = knuffel::parse("test.kdl", doc).unwrap();
        doc.validate().expect("validate failed");
        let invalid_enum = r#"
enum "Bar" {
  variant "b"
  variant "b" "u8"
}
"#;
        let doc: crate::Document = knuffel::parse("test.kdl", invalid_enum).unwrap();
        doc.validate().expect_err("validate failed");
        let invalid_type = r#"
enum "Bar" {
  variant "b" "Foo"
}
"#;
        let doc: crate::Document = knuffel::parse("test.kdl", invalid_type).unwrap();
        doc.validate().expect_err("validate failed");
    }
}
