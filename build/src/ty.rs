use miette::{miette, WrapErr};
use std::collections::HashMap;

use crate::{Expr, Ty};

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

    pub(crate) fn check_expr(&mut self, expr: &Expr) -> miette::Result<()> {
        let generic_tys = &expr.ty_def().generic_tys;
        let tys = expr.field_tys();
        for (name, ty) in &tys {
            self.check_ty(ty, generic_tys)
                .wrap_err(format!("{} error", name))?;
        }
        Ok(())
    }

    fn check_ty(&self, ty: &Ty, generic_tys: &[String]) -> miette::Result<()> {
        match ty {
            Ty::Array { ty, .. } => self.check_ty(ty, generic_tys),
            Ty::Unresolved { name, generic_args } => {
                if generic_tys.iter().any(|t| t == name) {
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
                for ty in generic_args {
                    self.check_ty(ty, generic_tys)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
