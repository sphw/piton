use miette::{miette, IntoDiagnostic, WrapErr};
use std::{alloc::Layout, collections::HashMap};

use crate::{Expr, GenericTy, Ty};

#[derive(Default)]
pub struct TyChecker {
    known_tys: HashMap<String, Expr>,
}

impl TyChecker {
    pub(crate) fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Bus(_) | Expr::Service(_) => return,
            _ => {}
        }
        let ty_def = expr.ty_def();
        self.known_tys.insert(ty_def.name.clone(), expr.clone());
    }

    pub(crate) fn resolve_expr(&mut self, expr: &mut Expr) -> miette::Result<()> {
        let generic_tys = expr.ty_def().generic_tys.clone();
        let tys = expr.field_tys();
        for (name, ty) in tys {
            self.resolve_ty(ty, &generic_tys)
                .wrap_err(format!("{} error", name))?;
        }
        Ok(())
    }

    fn resolve_ty(&self, ty: &mut Ty, generic_tys: &[GenericTy]) -> miette::Result<()> {
        match ty {
            Ty::Array { ty, .. } => self.resolve_ty(ty, generic_tys),
            Ty::Unresolved { name, generic_args } => {
                if generic_tys.iter().any(|t| t.name() == name) {
                    if !generic_args.is_empty() {
                        return Err(miette!("can't use generic args in this place"));
                    }
                    return Ok(());
                }
                let Some(resolved_ty) = self.known_tys.get(name.as_str()) else {
                    return Err(miette!("unknown type {}", name));
                };
                let ty_def = resolved_ty.ty_def();
                if ty_def.generic_tys.len() != generic_args.len() {
                    return Err(miette!(
                        "{} expects {} generic args not {}",
                        name,
                        ty_def.generic_tys.len(),
                        generic_args.len()
                    ));
                }
                for arg in generic_args.iter_mut() {
                    match arg {
                        crate::GenericArg::Ty(ty) => {
                            self.resolve_ty(ty, generic_tys)?;
                        }
                        crate::GenericArg::Const(_) => {}
                    }
                }

                if let Expr::Extern(e) = resolved_ty {
                    let map = e
                        .ty_def
                        .generic_tys
                        .iter()
                        .map(|t| t.name())
                        .zip(generic_args.iter())
                        .collect::<HashMap<_, _>>();
                    let Some(template) = e.concrete_impls.get("rust") else {
                        return Err(miette!("rust impl missing"));
                    };
                    let f = template
                        .iter()
                        .map(|t| match t {
                            crate::TemplateToken::Char(c) => c.to_string(),
                            crate::TemplateToken::Template(t) => {
                                map.get(t.as_str()).expect("invalid template key").to_rust()
                            }
                        })
                        .collect::<String>();
                    *ty = Ty::Extern(f);
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }
}

pub struct LayoutChecker {
    layout: Layout,
}

impl Default for LayoutChecker {
    fn default() -> Self {
        Self {
            layout: Layout::from_size_align(0, 1).expect("bad layout"),
        }
    }
}

impl LayoutChecker {
    pub fn next_field(&mut self, layout: Layout) -> miette::Result<()> {
        let last_offset = self.layout.size();
        let (new_layout, offset) = self.layout.extend(layout).into_diagnostic()?;
        if offset > last_offset {
            return Err(miette!(
                "types must have no internal padding required, try reordering your values: pad amount {}", offset - last_offset
            ));
        }
        self.layout = new_layout;
        Ok(())
    }

    pub fn final_pad(&self) -> usize {
        let align = 8;
        let len = self.layout.size();
        let len_rounded_up = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);
        len_rounded_up.wrapping_sub(len)
    }
}

impl Ty {
    pub fn layout(&self) -> Layout {
        match self {
            Ty::U64 => Layout::new::<u64>(),
            Ty::U32 => Layout::new::<u32>(),
            Ty::U16 => Layout::new::<u16>(),
            Ty::U8 => Layout::new::<u16>(),
            Ty::I64 => Layout::new::<i64>(),
            Ty::I32 => Layout::new::<i32>(),
            Ty::I16 => Layout::new::<i16>(),
            Ty::I8 => Layout::new::<i16>(),
            Ty::F64 => Layout::new::<f64>(),
            Ty::F32 => Layout::new::<f32>(),
            Ty::Bool => Layout::new::<bool>(),
            Ty::Array { ty, .. } => ty.layout(),
            Ty::Unresolved { .. } => Layout::from_size_align(8, 8).expect("bad layout"),
            Ty::Extern(_) => Layout::from_size_align(8, 8).expect("bad layout"),
        }
    }
}
