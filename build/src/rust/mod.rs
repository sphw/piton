use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use crate::{GenericArg, GenericTy, Ty};
use convert_case::{Case, Casing};
use genco::prelude::*;
use miette::IntoDiagnostic;

pub struct TypeGenerator;

impl super::TypeGenerator for TypeGenerator {
    fn generate_struct(&self, s: &crate::Struct) -> miette::Result<String> {
        let generic_tys =
            quote! { $(for t in &s.ty_def.generic_tys => $(t.to_rust()): piton::Yule + 'static,) };
        let generic_args: rust::Tokens = if s.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &s.ty_def.generic_tys => $(t.to_rust()))>
            }
        };
        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes, Clone, Debug, PartialEq, Eq, Default)]
            #[repr(C)]
            pub struct $(&s.ty_def.name) $(&generic_args) {
                $(for field in &s.fields => pub $(&field.name): $(ty_to_rust(&field.ty)),)
            }

            unsafe impl<$(generic_tys)> piton::Yule for $(&s.ty_def.name) $(generic_args) {}
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
        let first_var = e.variants.first().unwrap();
        let default_arg: rust::Tokens = if first_var.ty.is_some() {
            quote! {
                Self::$(first_var.name.to_case(Case::Pascal))(Default::default())
            }
        } else {
            quote! {
                Self::$(first_var.name.to_case(Case::Pascal))
            }
        };

        let generic_args: rust::Tokens = if e.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &e.ty_def.generic_tys => $(t.to_rust()))>
            }
        };
        let generic_tys =
            quote! { $(for t in &e.ty_def.generic_tys => $(t.to_rust()): piton::Yule + 'static,) };

        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes, Clone, Debug)]
            #[repr(u8)]
            pub enum $(&e.ty_def.name)$(&generic_args) {
                $(for t in vars => $(t))
            }

            impl<$(&generic_tys)> core::default::Default for $(&e.ty_def.name)$(&generic_args) {
                fn default() -> Self {
                    $(default_arg)
                }
            }

            unsafe impl<$(generic_tys)> piton::Yule for $(&e.ty_def.name) $(generic_args) {}
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
        let trait_methods: Vec<rust::Tokens> = service.methods.iter().map(|method| {
            quote! {
                fn $(method.name.to_case(Case::Snake))(&mut self, msg: &$(ty_to_rust(&method.arg_ty)), resp: &mut $(ty_to_rust(&method.return_ty))) -> Result<(), piton::Error>;
            }
        }).collect();

        let match_arms: Vec<rust::Tokens> = service
            .methods
            .iter()
            .map(|method| {
                quote! {
                    $(&pascal_name)Req::$(method.name.to_case(Case::Pascal))(arg) => {
                        {
                            *recv.resp = $(&pascal_name)Ret::$(method.name.to_case(Case::Pascal))(Default::default());
                            #[allow(irrefutable_let_patterns)]
                            let $(&pascal_name)Ret::$(method.name.to_case(Case::Pascal))(resp) = &mut *recv.resp else {
                                unreachable!()
                            };
                            self.service.$(method.name.to_case(Case::Snake))(arg, resp)?;
                        }
                        recv.responder.send(recv.resp)?;
                    }
                }
            })
            .collect();

        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                $(for t in &service.ty_def.generic_tys => $(t.to_rust()))
            }
        };

        let phantom_tys = phantom_tys(&service.ty_def.generic_tys);
        let phantom_new = phantom_new(&service.ty_def.generic_tys);

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): piton::Yule + 'static,) };

        let tokens: rust::Tokens = quote! {
            pub trait $(&pascal_name)Service<T: piton::ServiceRx, $(&generic_tys)> {
                $(for method in trait_methods => $(method))
            }

            pub struct $(&pascal_name)Server<T, S, $(&generic_tys)> {
                transport: T,
                service: S,
                $(phantom_tys)
            }


            impl<T: piton::ServiceRx, S: $(&pascal_name)Service<T, $(&generic_args)>, $(&generic_tys)> $(&pascal_name)Server<T, S, $(&generic_args)> {
                pub fn new(transport: T, service: S) -> Self {
                    Self {
                        transport,
                        service,
                        $(phantom_new)
                    }
                }

                pub fn run(mut self) -> Result<(), piton::Error> {
                    use piton::Responder;
                    while let Some(mut recv) = self.transport.recv::<$(&pascal_name)Ret<$(&generic_args)>, $(&pascal_name)Req<$(&generic_args)>>()? {
                        use piton::BufR;
                        #[allow(clippy::single_match, unreachable_patterns)]
                        match recv.req.as_ref() {
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
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                $(for t in &service.ty_def.generic_tys => $(t.to_rust()))
            }
        };

        let generic_enum_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &service.ty_def.generic_tys => $(t.to_rust()))>
            }
        };

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): piton::Yule  + 'static,) };

        let methods: Vec<rust::Tokens> = service.methods.iter().map(|method| {
            let method_pascal = method.name.to_case(Case::Pascal);

            quote! {
                pub fn $(method.name.to_case(Case::Snake))_ref(&mut self) -> Result<$(&method_pascal)CallRef<'_, T, $(&generic_args)>, piton::Error> {
                    Ok($(method_pascal)CallRef {
                        msg: self.transport.alloc()?,
                        transport: &mut self.transport,
                    })
                }
            }
        }).collect();

        let method_structs: Vec<rust::Tokens> = service
            .methods
            .iter()
            .map(|method| {
                let arg_ty = &method.arg_ty;
                let return_ty = &method.return_ty;
                let method_pascal = method.name.to_case(Case::Pascal);
                let req_enum = quote! { $(&pascal_name)Req };
                let ret_enum = quote! { $(&pascal_name)Ret };

                quote! {
                    pub struct $(&method_pascal)CallRef<'a, Serv: piton::ServiceTx + 'a, $(&generic_tys)> {
                        transport: &'a mut Serv,
                        msg: <Serv as piton::ServiceTx>::BufW<'a, $(&pascal_name)Req$(&generic_enum_args)>,
                    }

                    impl<'a, S: piton::ServiceTx + 'a, $(&generic_tys)> $(&method_pascal)CallRef<'a, S, $(&generic_args)> {
                        pub fn call(self) -> Result<$(&method_pascal)RetRef<'a, S, $(&generic_args)>, piton::Error> {
                            let msg = self.transport.call(self.msg)?;
                            Ok($(&method_pascal)RetRef { msg })
                        }
                    }

                    impl<'a, S: piton::ServiceTx + 'a, $(&generic_tys)> core::ops::Deref for $(&method_pascal)CallRef<'a, S, $(&generic_args)> {
                        type Target = $(ty_to_rust(arg_ty));

                        fn deref(&self) -> &Self::Target {
                            #[allow(irrefutable_let_patterns)]
                            if let $(&req_enum)::$(&method_pascal)(v) = self.msg.deref() {
                                v
                            }else { unreachable!() }
                        }
                    }

                    impl<'a, S: piton::ServiceTx + 'a, $(&generic_tys)> core::ops::DerefMut for $(&method_pascal)CallRef<'a, S, $(&generic_args)> {
                        fn deref_mut(&mut self) -> &mut Self::Target {
                            #[allow(irrefutable_let_patterns)]
                            if let $(req_enum)::$(&method_pascal)(v) = self.msg.deref_mut() {
                                v
                            }else { unreachable!() }
                        }
                    }

                    pub struct $(&method_pascal)RetRef<'a, Serv: piton::ServiceTx + 'a, $(&generic_tys)> {
                        msg: <Serv as piton::ServiceTx>::BufR<'a, $(&pascal_name)Ret$(&generic_enum_args)>,
                    }

                    impl<'a, S: piton::ServiceTx + 'a, $(&generic_tys)> core::ops::Deref for $(&method_pascal)RetRef<'a, S, $(&generic_args)> {
                        type Target = $(ty_to_rust(return_ty));

                        fn deref(&self) -> &Self::Target {
                            #[allow(irrefutable_let_patterns)]
                            if let $(&ret_enum)::$(&method_pascal)(v) = self.msg.deref() {
                                v
                            }else { unreachable!() }
                        }
                    }

                }
            })
            .collect();

        let phantom_tys = phantom_tys(&service.ty_def.generic_tys);
        let phantom_new = phantom_new(&service.ty_def.generic_tys);

        let tokens: rust::Tokens = quote! {
            $(for method in method_structs => $(method))

            pub struct $(&pascal_name)Client<T, $(&generic_tys)> {
                pub transport: T,
                $(phantom_tys)
            }

            impl<T: piton::ServiceTx, $(&generic_tys)> $(&pascal_name)Client<T, $(&generic_args)> {
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
        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &service.ty_def.generic_tys => $(t.to_rust()): piton::Yule)>
            }
        };
        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust())) };

        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes)]
            #[repr(u32)]
            pub enum $(&pascal_name)Req$(&generic_args) {
                $(for method in service.methods.iter() => $(method.name.to_case(Case::Pascal))($(ty_to_rust(&method.arg_ty))),)
            }
            impl$(&generic_args) core::default::Default for $(&pascal_name)Req<$(&generic_tys)> {
                fn default() -> Self { todo!() }
            }

            unsafe impl$(&generic_args) piton::Yule for $(&pascal_name)Req<$(&generic_tys)> {}

            #[derive(bytecheck::CheckBytes)]
            #[repr(u32)]
            pub enum $(&pascal_name)Ret$(&generic_args) {
                $(for method in service.methods.iter() => $(method.name.to_case(Case::Pascal))($(ty_to_rust(&method.return_ty))),)
            }

            impl$(&generic_args) core::default::Default for $(&pascal_name)Ret<$(&generic_tys)> {
                fn default() -> Self { todo!() }
            }

            unsafe impl$(&generic_args) piton::Yule for $(&pascal_name)Ret<$(&generic_tys)> {}
        };
        Ok(tokens.to_file_string().unwrap())
    }
}

pub struct MsgGenerator;
impl crate::BusGenerator for MsgGenerator {
    fn generate_bus(&self, bus: &crate::Bus) -> miette::Result<String> {
        let pascal_name = bus.ty_def.name.to_case(Case::Pascal);
        let generic_args: rust::Tokens = if bus.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                <$(for t in &bus.ty_def.generic_tys => $(t.to_rust()): piton::Yule)>
            }
        };
        let generic_tys = quote! { $(for t in &bus.ty_def.generic_tys => $(t.to_rust()),) };

        let tokens: rust::Tokens = quote! {
            #[derive(bytecheck::CheckBytes)]
            #[repr(u32)]
            pub enum $(&pascal_name)Msg$(&generic_args){
                $(for method in bus.msgs.iter() => $(method.name.to_case(Case::Pascal))($(ty_to_rust(&method.ty))),)
            }

            impl$(&generic_args) core::default::Default for $(&pascal_name)Msg<$(&generic_tys)> {
                fn default() -> Self { todo!() }
            }

            unsafe impl$(generic_args) piton::Yule for $(&pascal_name)Msg<$(&generic_tys)> {}
        };
        Ok(tokens.to_file_string().unwrap())
    }
}

pub struct BusTxGenerator;

impl crate::BusGenerator for BusTxGenerator {
    fn generate_bus(&self, service: &crate::Bus) -> miette::Result<String> {
        let mut hasher = DefaultHasher::default();
        hasher.write(service.ty_def.name.as_bytes());
        let pascal_name = service.ty_def.name.to_case(Case::Pascal);
        let methods: Vec<rust::Tokens> = service.msgs.iter().map(|method| {
            let arg_ty = &method.ty;

            quote! {
                pub fn $(method.name.to_case(Case::Snake))(&mut self, msg: T::BufW<'_, $(ty_to_rust(arg_ty))>) -> Result<(), piton::Error> {
                    self.transport.send(msg)
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

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): piton::Yule + 'static,) };

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
        let trait_methods: Vec<rust::Tokens> = service.msgs.iter().map(|method| {
            quote! {
                fn $(method.name.to_case(Case::Snake))(&mut self, msg: & $(ty_to_rust(&method.ty))) -> Result<(), piton::Error>;
            }
        }).collect();

        let generic_args: rust::Tokens = if service.ty_def.generic_tys.is_empty() {
            quote! {}
        } else {
            quote! {
                $(for t in &service.ty_def.generic_tys => $(t.to_rust()))
            }
        };

        let generic_tys = quote! { $(for t in &service.ty_def.generic_tys => $(t.to_rust()): piton::Yule + 'static,) };

        let match_arms: Vec<rust::Tokens> = service
            .msgs
            .iter()
            .map(|method| {
                quote! {
                    $(&pascal_name)Msg::$(method.name.to_case(Case::Pascal))(arg) => {
                        self.service.$(method.name.to_case(Case::Snake))(arg)?;
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

                pub fn run(mut self) -> Result<(), piton::Error> {
                    while let Some(recv) = self.transport.recv::<$(&pascal_name)Msg<$(&generic_args)>>()? {
                        use piton::BufR;
                        #[allow(clippy::single_match, unreachable_patterns)]
                        match recv.as_ref() {
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


            impl<T: piton::BusRx, $(&generic_tys)> $(&pascal_name)Rx<T, $(&generic_args)> {
                pub fn new(transport: T) -> Self {
                    Self {
                        transport,
                        $(phantom_new)
                    }
                }

                pub fn recv(&mut self) -> Result<Option<T::BufR<'_, $(&pascal_name)Msg<$(&generic_args)>>>, piton::Error> {
                     self.transport.recv::<$(&pascal_name)Msg<$(&generic_args)>>()
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
