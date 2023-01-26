use genco::quote;
use miette::IntoDiagnostic;

use crate::{GenericArg, GenericTy, Ty};

pub struct TypeGenerator;

type Cpp = genco::lang::Csharp;
type Tokens = genco::Tokens<Cpp>;

impl super::TypeGenerator for TypeGenerator {
    fn generate_struct(&self, s: &crate::Struct) -> miette::Result<String> {
        let template_def: Tokens = if s.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                template <$(for t in &s.ty_def.generic_tys => $(t.to_cpp()))
            }
        };
        let tokens: Tokens = quote! {
            $(template_def)
            struct $(&s.ty_def.name) {
                $(for field in &s.fields => $(field.ty.to_cpp()) $(&field.name);)
            };
        };
        tokens.to_file_string().into_diagnostic()
    }

    fn generate_enum(&self, e: &crate::Enum) -> miette::Result<String> {
        todo!()
    }
}

impl GenericArg {
    pub fn to_cpp(&self) -> String {
        match self {
            GenericArg::Ty(ty) => ty.to_cpp(),
            GenericArg::Const(ty) => {
                format!("{}", ty)
            }
        }
    }
}

impl GenericTy {
    pub fn to_cpp(&self) -> String {
        match self {
            GenericTy::Ty(ty) => ty.clone(),
            GenericTy::Const { ty, name } => {
                format!("{} {}", ty.to_cpp(), name)
            }
        }
    }
}

impl Ty {
    pub fn to_cpp(&self) -> String {
        match self {
            Ty::U64 => "uint64_t".to_string(),
            Ty::U32 => "uint32_t".to_string(),
            Ty::U16 => "uint16_t".to_string(),
            Ty::U8 => "uint8_t".to_string(),
            Ty::I64 => "int64_t".to_string(),
            Ty::I32 => "int32_t".to_string(),
            Ty::I16 => "int16_t".to_string(),
            Ty::I8 => "int8_t".to_string(),
            Ty::F32 => "float".to_string(),
            Ty::F64 => "double".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Array { ty, len } => todo!(),
            Ty::Unresolved { name, generic_args } => todo!(),
            Ty::Extern(e) => e.to_string(),
        }
    }
}
