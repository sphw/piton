use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use crate::Ty;
use convert_case::{Case, Casing};
use genco::prelude::*;
use miette::IntoDiagnostic;

pub struct TypeGenerator;

impl super::TypeGenerator for TypeGenerator {
    fn generate_struct(&self, s: &crate::Struct) -> miette::Result<String> {
        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes, Clone, Debug)]
            #[repr(C)]
            pub struct $(&s.name) {
                $(for field in &s.fields => pub $(&field.name): $(ty_to_rust(&field.ty)),)
            }
        };
        tokens.to_file_string().into_diagnostic()
    }

    fn generate_enum(&self, e: &crate::Enum) -> miette::Result<String> {
        let vars: Vec<rust::Tokens> = e
            .variants
            .iter()
            .map(|var| {
                if let Some(ty) = &var.ty {
                    quote! {
                        $(var.name.to_case(Case::Pascal))($(ty_to_rust(ty))),
                    }
                } else {
                    quote! {
                        $(var.name.to_case(Case::Pascal)),
                    }
                }
            })
            .collect();
        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes, Clone, Debug)]
            #[repr(u8)]
            pub enum $(&e.name) {
                $(for t in vars => $(t))
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

fn ty_to_rust(ty: &Ty) -> String {
    match ty {
        Ty::U64 => "u64".to_string(),
        Ty::U32 => "u32".to_string(),
        Ty::U16 => "u16".to_string(),
        Ty::U8 => "u8".to_string(),
        Ty::I64 => "i64".to_string(),
        Ty::I32 => "i32".to_string(),
        Ty::I16 => "i16".to_string(),
        Ty::I8 => "i8".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Array { ty, len } => format!("[{}; {}]", ty_to_rust(ty), len),
        Ty::Unresolved(ty) => ty.to_string(),
    }
}

pub struct ServiceGenerator;

impl crate::ServiceGenerator for ServiceGenerator {
    fn generate_service(&self, service: &crate::Service) -> miette::Result<String> {
        let pascal_name = service.name.to_case(Case::Pascal);
        let mut hasher = DefaultHasher::default();
        hasher.write(service.name.as_bytes());
        let addr = hasher.finish() as u32 & 0xFF000000;
        let trait_methods: Vec<rust::Tokens> = service.methods.iter().map(|method| {
            let args = if let Some(arg_ty) = &method.arg_ty {
                quote! { ,msg: &mut $(ty_to_rust(arg_ty)) }
            } else {
                quote! {}
            };

            let return_arg= if let Some(return_ty) = &method.return_ty {
                let ty = ty_to_rust(return_ty);
                quote! { ,resp: &mut piton::TypedBuf<T::BufW<'id>, $(ty)> }
            } else {
                quote! {}
            };

            let return_ty = if let Some(return_ty) = &method.return_ty {
                let ty = ty_to_rust(return_ty);
                quote! { piton::InsertToken<T::BufW<'id>, $(ty)> }
            } else {
                quote! { () }
            };

            quote! {
                fn $(method.name.to_case(Case::Snake))<'id>(&mut self $(args) $(return_arg)) -> Result<$(return_ty), T::Error>;
            }
        }).collect();

        let match_arms: Vec<rust::Tokens> = service
            .methods
            .iter()
            .enumerate()
            .map(|(i, method)| {
                let args = if method.arg_ty.is_some() {
                    quote! { recv.req.as_mut().unwrap(), }
                } else {
                    quote! {}
                };

                let return_ty = if let Some(return_ty) = &method.return_ty {
                    quote! { $(ty_to_rust(return_ty)) }
                } else {
                    quote! {()}
                };

                let return_arg = if method.return_ty.is_some() {
                    quote! { &mut resp }
                } else {
                    quote! {}
                };

                quote! {
                    $(i as u32 | addr) => {
                        let mut resp: piton::TypedBuf<T::BufW<'_>, $(return_ty)> = piton::TypedBuf::new(recv.resp).unwrap();
                        self.service.$(method.name.to_case(Case::Snake))($(args) $(return_arg))?;
                        recv.responder.send(msg_type, resp)?;
                    }
                }
            })
            .collect();

        let tokens: rust::Tokens = quote! {
            pub trait $(&pascal_name)Service<T: piton::ServerTransport> {
                $(for method in trait_methods => $(method))
            }

            pub struct $(&pascal_name)Server<T, S> {
                pub transport: T,
                pub service: S,
            }

            impl<T: piton::ServerTransport, S: $(&pascal_name)Service<T>> $(pascal_name)Server<T, S> {
                pub fn run(mut self) -> Result<(), T::Error> {
                    use piton::{Responder, Buf};
                    while let Some(mut recv) = self.transport.recv()? {
                        let msg_type = recv.msg_type;
                        #[allow(clippy::single_match)]
                        match msg_type {
                            $(for arm in match_arms => $(arm))
                            _ => {}
                        }
                    }
                    Ok(())
                }
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

pub struct ClientGenerator;

impl crate::ServiceGenerator for ClientGenerator {
    fn generate_service(&self, service: &crate::Service) -> miette::Result<String> {
        let mut hasher = DefaultHasher::default();
        hasher.write(service.name.as_bytes());
        let addr = hasher.finish() as u32 & 0xFF000000;
        let pascal_name = service.name.to_case(Case::Pascal);
        let methods: Vec<rust::Tokens> = service.methods.iter().map(|method| {
            let args = if let Some(arg_ty) = &method.arg_ty {
                quote! { msg: piton::TypedBuf<T::BufW<'_>, $(ty_to_rust(arg_ty))>, }
            } else {
                quote! {}
            };


            let call_arg: rust::Tokens = if method.arg_ty.is_some() {
                quote! { , msg }
            }else { quote! {} };

            let return_ty = if let Some(return_ty) = &method.return_ty {
                quote! { piton::TypedBuf<T::BufR<'_>, $(ty_to_rust(return_ty))> }
            } else {
                quote! { () }
            };

            quote! {
                pub fn $(method.name.to_case(Case::Snake))(&mut self, $(args)) -> Result<$(return_ty), T::Error> {
                    self.transport.call($(&pascal_name)Req::$(method.name.to_case(Case::Pascal)) as u32 | $(addr) $(call_arg))
                }
            }
        }).collect();
        let tokens: rust::Tokens = quote! {
            pub struct $(&pascal_name)Client<T> {
                pub transport: T
            }

            impl<T: piton::ClientTransport> $(&pascal_name)Client<T> {
                $(for method in methods => $(method))
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

pub struct ReqGenerator;
impl crate::ServiceGenerator for ReqGenerator {
    fn generate_service(&self, service: &crate::Service) -> miette::Result<String> {
        let pascal_name = service.name.to_case(Case::Pascal);
        let tokens: rust::Tokens = quote! {
            #[repr(u32)]
            pub enum $(&pascal_name)Req {
                $(for (i, method) in service.methods.iter().enumerate() => $(method.name.to_case(Case::Pascal)) = $(i),)
            }
        };
        Ok(tokens.to_file_string().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Field, Struct, Ty, TypeGenerator};

    #[test]
    fn test_struct_gen() {
        let code = super::TypeGenerator
            .generate_struct(&Struct {
                name: "Test".to_string(),
                fields: vec![
                    Field {
                        name: "foo".to_string(),
                        ty: Ty::U16,
                    },
                    Field {
                        name: "bar".to_string(),
                        ty: Ty::U32,
                    },
                    Field {
                        name: "boolean".to_string(),
                        ty: Ty::Bool,
                    },
                    Field {
                        name: "array".to_string(),
                        ty: Ty::Array {
                            ty: Box::new(Ty::U8),
                            len: 20,
                        },
                    },
                ],
            })
            .expect("struct gen failed");
        assert_eq!(
            code,
            r#"#[derive(bytecheck::CheckBytes, Clone, Debug)] #[repr(C)] pub struct Test { pub foo: u16,pub bar: u32,pub boolean: bool,pub array: [u8; 20], }
"#
        );
    }
}
