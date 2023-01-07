use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use crate::{GenericArg, GenericTy, Ty};
use convert_case::{Case, Casing};
use genco::prelude::*;
use miette::IntoDiagnostic;

pub struct TypeGenerator;

impl super::TypeGenerator for TypeGenerator {
    fn generate_struct(&self, s: &crate::Struct) -> miette::Result<String> {
        let generic_args: rust::Tokens = if s.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &s.ty_def.generic_tys => $(t.to_rust()))>
            }
        };
        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes, Clone, Debug, PartialEq)]
            #[repr(C)]
            pub struct $(&s.ty_def.name) $(generic_args) {
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
        let generic_args: rust::Tokens = if e.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &e.ty_def.generic_tys => $(t.to_rust()))>
            }
        };
        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes, Clone, Debug)]
            #[repr(u8)]
            pub enum $(&e.ty_def.name)$(generic_args) {
                $(for t in vars => $(t))
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

impl GenericArg {
    pub fn to_rust(&self) -> String {
        match self {
            GenericArg::Ty(ty) => ty_to_rust(ty),
            GenericArg::Const(ty) => format!("{}", ty),
        }
    }
}

impl GenericTy {
    pub fn to_rust(&self) -> String {
        match self {
            GenericTy::Ty(ty) => ty.clone(),
            GenericTy::Const { ty, name } => format!("const {}: {}", name, ty_to_rust(ty)),
        }
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
        Ty::Unresolved { name, generic_args } => {
            let args = if generic_args.is_empty() {
                "".to_string()
            } else {
                format!(
                    "<{}>",
                    generic_args
                        .iter()
                        .map(GenericArg::to_rust)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            format!("{}{}", name, args)
        }
        Ty::F32 => "f32".to_string(),
        Ty::F64 => "f64".to_string(),
        Ty::Extern(e) => e.clone(),
    }
}

pub struct ServiceGenerator;

impl crate::ServiceGenerator for ServiceGenerator {
    fn generate_service(&self, service: &crate::Service) -> miette::Result<String> {
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let mut hasher = DefaultHasher::default();
        hasher.write(service.ty_def.name.as_bytes());
        let addr = hasher.finish() as u32 & 0xFF000000;
        let trait_methods: Vec<rust::Tokens> = service.methods.iter().map(|method| {
            quote! {
                fn $(method.name.to_case(Case::Snake))<'id>(&mut self, msg: &mut $(ty_to_rust(&method.arg_ty)), resp: &mut piton::TypedBuf<T::BufW<'id>, $(ty_to_rust(&method.return_ty))>) -> Result<piton::InsertToken<T::BufW<'id>, $(ty_to_rust(&method.return_ty))>, T::Error>;
            }
        }).collect();

        let match_arms: Vec<rust::Tokens> = service
            .methods
            .iter()
            .enumerate()
            .map(|(i, method)| {
                quote! {
                    $(i as u32 | addr) => {
                        let mut resp: piton::TypedBuf<T::BufW<'_>, $(ty_to_rust(&method.return_ty)) > = piton::TypedBuf::new(recv.resp).unwrap();
                        self.service.$(method.name.to_case(Case::Snake))(recv.req.as_mut().unwrap(), &mut resp)?;
                        recv.responder.send(msg_type, resp)?;
                    }
                }
            })
            .collect();

        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                , $(for t in &service.ty_def.generic_tys => $(t.to_rust()))
            }
        };

        let phantom_tys = phantom_tys(&service.ty_def.generic_tys);
        let phantom_new = phantom_new(&service.ty_def.generic_tys);

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): bytecheck::CheckBytes<()> + 'static,) };

        let tokens: rust::Tokens = quote! {
            pub trait $(&pascal_name)Service<T: piton::ServiceRx, $(&generic_tys)> {
                $(for method in trait_methods => $(method))
            }

            pub struct $(&pascal_name)Server<T, S, $(&generic_tys)> {
                transport: T,
                service: S,
                $(phantom_tys)
            }


            impl<T: piton::ServiceRx, S: $(&pascal_name)Service<T $(&generic_args)>, $(&generic_tys)> $(&pascal_name)Server<T, S $(&generic_args)> {
                pub fn new(transport: T, service: S) -> Self {
                    Self {
                        transport,
                        service,
                        $(phantom_new)
                    }
                }

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
        hasher.write(service.ty_def.name.as_bytes());
        let addr = hasher.finish() as u32 & 0xFF000000;
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let methods: Vec<rust::Tokens> = service.methods.iter().map(|method| {
            let arg_ty = &method.arg_ty;
            let return_ty = &method.return_ty;

            quote! {
                pub fn $(method.name.to_case(Case::Snake))(&mut self, msg: piton::TypedBuf<T::BufW<'_>, $(ty_to_rust(arg_ty))>) -> Result<piton::TypedBuf<T::BufR<'_>, $(ty_to_rust(return_ty))>, T::Error> {
                    self.transport.call($(&pascal_name)Req::$(method.name.to_case(Case::Pascal)) as u32 | $(addr), msg )
                }
            }
        }).collect();
        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                , $(for t in &service.ty_def.generic_tys => $(t.to_rust()))
            }
        };

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): bytecheck::CheckBytes<()> + 'static,) };

        let phantom_tys = phantom_tys(&service.ty_def.generic_tys);
        let phantom_new = phantom_new(&service.ty_def.generic_tys);

        let tokens: rust::Tokens = quote! {
            pub struct $(&pascal_name)Client<T, $(&generic_tys)> {
                pub transport: T,
                $(phantom_tys)
            }

            impl<T: piton::ServiceTx, $(&generic_tys)> $(&pascal_name)Client<T $(&generic_args)> {
                pub fn new(transport: T) -> Self {
                    Self {
                        transport,
                        $(phantom_new)
                    }
                }

                $(for method in methods => $(method))
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

pub struct ReqGenerator;
impl crate::ServiceGenerator for ReqGenerator {
    fn generate_service(&self, service: &crate::Service) -> miette::Result<String> {
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let tokens: rust::Tokens = quote! {
            #[repr(u32)]
            pub enum $(&pascal_name)Req {
                $(for (i, method) in service.methods.iter().enumerate() => $(method.name.to_case(Case::Pascal)) = $(i),)
            }
        };
        Ok(tokens.to_file_string().unwrap())
    }
}

pub struct MsgGenerator;
impl crate::BusGenerator for MsgGenerator {
    fn generate_bus(&self, bus: &crate::Bus) -> miette::Result<String> {
        let pascal_name = bus.ty_def.name.to_case(Case::Pascal);
        let tokens: rust::Tokens = quote! {
            #[repr(u32)]
            pub enum $(&pascal_name)Msg {
                $(for (i, method) in bus.msgs.iter().enumerate() => $(method.name.to_case(Case::Pascal)) = $(i),)
            }
        };
        Ok(tokens.to_file_string().unwrap())
    }
}

pub struct BusTxGenerator;

impl crate::BusGenerator for BusTxGenerator {
    fn generate_bus(&self, service: &crate::Bus) -> miette::Result<String> {
        let mut hasher = DefaultHasher::default();
        hasher.write(service.ty_def.name.as_bytes());
        let addr = hasher.finish() as u32 & 0xFF000000;
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let methods: Vec<rust::Tokens> = service.msgs.iter().map(|method| {
            let arg_ty = &method.ty;

            quote! {
                pub fn $(method.name.to_case(Case::Snake))(&mut self, msg: piton::TypedBuf<T::BufW<'_>, $(ty_to_rust(arg_ty))>) -> Result<(), T::Error> {
                    self.transport.send($(&pascal_name)Msg::$(method.name.to_case(Case::Pascal)) as u32 | $(addr), msg )
                }
            }
        }).collect();

        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! { <T> }
        } else {
            quote! {
                <T, $(for t in &service.ty_def.generic_tys => $(t.to_rust()))>
            }
        };

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): bytecheck::CheckBytes<()> + 'static,) };

        let phantom_tys = phantom_tys(&service.ty_def.generic_tys);
        let phantom_new = phantom_new(&service.ty_def.generic_tys);

        let tokens: rust::Tokens = quote! {
            pub struct $(&pascal_name)Client$(&generic_args) {
                pub transport: T,
                $(phantom_tys)
            }

            impl<T: piton::BusTx, $(generic_tys)> $(&pascal_name)Client$(&generic_args) {
                pub fn new(transport: T) -> Self {
                    Self {
                        transport,
                        $(phantom_new)
                    }
                }

                $(for method in methods => $(method))
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

pub struct BusRxGenerator;

impl crate::BusGenerator for BusRxGenerator {
    fn generate_bus(&self, service: &crate::Bus) -> miette::Result<String> {
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let mut hasher = DefaultHasher::default();
        hasher.write(service.ty_def.name.as_bytes());
        let addr = hasher.finish() as u32 & 0xFF000000;
        let trait_methods: Vec<rust::Tokens> = service.msgs.iter().map(|method| {
            quote! {
                fn $(method.name.to_case(Case::Snake))(&mut self, msg: &mut $(ty_to_rust(&method.ty))) -> Result<(), T::Error>;
            }
        }).collect();

        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                $(for t in &service.ty_def.generic_tys => $(t.to_rust()))
            }
        };

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): bytecheck::CheckBytes<()> + 'static,) };

        let match_arms: Vec<rust::Tokens> = service
            .msgs
            .iter()
            .enumerate()
            .map(|(i, method)| {
                quote! {
                    $(i as u32 | addr) => {
                        self.service.$(method.name.to_case(Case::Snake))(recv.req.as_mut().unwrap())?;
                    }
                }
            })
            .collect();

        let rx_match_arms: Vec<rust::Tokens> = service
            .msgs
            .iter()
            .enumerate()
            .map(|(i, method)| {
                quote! {
                    $(i as u32 | addr) => {
                      $(&pascal_name)MsgRx::$(method.name.to_case(Case::Pascal))(piton::TypedBuf::new(recv.req).unwrap())
                    }
                }
            })
            .collect();

        let phantom_tys = phantom_tys(&service.ty_def.generic_tys);
        let phantom_new = phantom_new(&service.ty_def.generic_tys);

        let tokens: rust::Tokens = quote! {
            pub trait $(&pascal_name)Service<T: piton::BusRx, $(&generic_args)> {
                $(for method in trait_methods => $(method))
            }

            pub struct $(&pascal_name)Server<T, S, $(&generic_tys)> {
                pub transport: T,
                pub service: S,
                $(phantom_tys.clone())
            }

            impl<T: piton::BusRx, S: $(&pascal_name)Service<T, $(&generic_args)>, $(&generic_tys)> $(&pascal_name)Server<T, S, $(&generic_args)> {
                pub fn new(transport: T, service: S) -> Self {
                    Self {
                        transport,
                        service,
                        $(phantom_new.clone())
                    }
                }

                pub fn run(mut self) -> Result<(), T::Error> {
                    use piton::Buf;
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


            pub struct $(&pascal_name)Rx<T, $(&generic_tys)> {
                pub transport: T,
                $(phantom_tys)
            }

            pub enum $(&pascal_name)MsgRx<B, $(&generic_args)> {
                $(for msg in service.msgs.iter() => $(msg.name.to_case(Case::Pascal))(piton::TypedBuf<B, $(ty_to_rust(&msg.ty))>),)
            }

            impl<T: piton::BusRx, $(&generic_tys)> $(&pascal_name)Rx<T, $(&generic_args)> {
                pub fn new(transport: T) -> Self {
                    Self {
                        transport,
                        $(phantom_new)
                    }
                }

                pub fn recv(&mut self) -> Result<$(&pascal_name)MsgRx<T::BufR<'_>, $(&generic_args)>, T::Error> {
                    Ok(if let Some(recv) = self.transport.recv()? {
                        let msg_type = recv.msg_type;
                        #[allow(clippy::single_match)]
                        match msg_type {
                            $(for arm in rx_match_arms => $(arm))
                            _ => {todo!()}
                        }
                    }else { todo!() })
                }
            }
        };
        tokens.to_file_string().into_diagnostic()
    }
}

fn phantom_tys(generic_tys: &[GenericTy]) -> Vec<rust::Tokens> {
    generic_tys
        .iter()
        .map(|t| match t {
            GenericTy::Ty(t) => {
                quote! { phantom_$(t.to_case(Case::Snake)): core::marker::PhantomData<$(t)> }
            }
            GenericTy::Const { .. } => {
                quote! {}
            }
        })
        .collect()
}

fn phantom_new(generic_tys: &[GenericTy]) -> Vec<rust::Tokens> {
    generic_tys
        .iter()
        .map(|t| match t {
            GenericTy::Ty(t) => {
                quote! { phantom_$(t.to_case(Case::Snake)): Default::default() }
            }
            GenericTy::Const { .. } => {
                quote! {}
            }
        })
        .collect()
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
