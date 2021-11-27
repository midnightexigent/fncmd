use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_str, Attribute, Block, Ident, ItemFn, LitStr, ReturnType, Visibility};

use super::{FncmdArg, FncmdAttr, FncmdSubcmds};

#[allow(dead_code)]
pub struct Fncmd {
	pub name: String,
	pub documentation: Option<TokenStream>,
	pub attributes: Vec<Attribute>,
	pub args: Vec<FncmdArg>,
	pub return_type: ReturnType,
	pub body: Box<Block>,
	pub visibility: Visibility,
	pub subcmds: FncmdSubcmds,
	pub version: String,
}

impl Fncmd {
	pub fn parse(
		self_name: String,
		self_version: String,
		_: FncmdAttr,
		item: &ItemFn,
		subcmds: FncmdSubcmds,
	) -> Fncmd {
		let fn_attrs = item.attrs.iter();
		let fn_vis = &item.vis;
		let fn_args = item.sig.inputs.iter();
		let fn_ret = &item.sig.output;
		let fn_body = &item.block;

		let mut fn_doc = None;
		let mut fncmd_attrs: Vec<Attribute> = Vec::new();

		for attr in fn_attrs {
			if attr.path.is_ident("doc") {
				fn_doc = Some(quote! { #attr })
			} else {
				fncmd_attrs.push(attr.clone());
			}
		}

		let fncmd_args: Vec<FncmdArg> = fn_args.map(FncmdArg::parse).collect();

		Fncmd {
			name: self_name,
			documentation: fn_doc,
			attributes: fncmd_attrs,
			args: fncmd_args,
			return_type: fn_ret.clone(),
			body: fn_body.clone(),
			visibility: fn_vis.clone(),
			subcmds,
			version: self_version,
		}
	}
}

impl From<Fncmd> for proc_macro::TokenStream {
	fn from(from: Fncmd) -> proc_macro::TokenStream {
		quote!(#from).into()
	}
}

impl ToTokens for Fncmd {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let Fncmd {
			name: cmd_name,
			documentation,
			attributes: attrs,
			args,
			return_type,
			body,
			visibility,
			subcmds,
			version,
			..
		} = self;

		let doc = quote!(#documentation);

		let vars: Vec<TokenStream> = args
			.into_iter()
			.map(|arg| {
				let name = &arg.name;
				let mutability = &arg.mutability;
				quote!(#mutability #name)
			})
			.collect();

		let (subcmd_imports, subcmd_patterns): (Vec<_>, Vec<_>) = subcmds
			.iter()
			.map(|(name, (_, path))| {
				let subcmd_name = name.strip_prefix(cmd_name).unwrap();
				let snake_case_name = subcmd_name.to_snake_case();
				let enumitem_name: Ident = parse_str(&snake_case_name).unwrap();
				let mod_name: Ident =
					parse_str(&format!("__fncmd_mod_{}", snake_case_name)).unwrap();
				let path_str: LitStr =
					parse_str(&format!(r#""{}""#, path.to_str().unwrap())).unwrap();
				let import = quote! {
					#[path = #path_str]
					mod #mod_name;
				};
				let enumitem = quote! {
					#enumitem_name(#mod_name::__fncmd_options)
				};
				let case = quote! {
					__fncmd_subcmds::#enumitem_name(__fncmd_options) => {
						#mod_name::__fncmd_exec(Some(__fncmd_options)).into()
					}
				};
				(import, (enumitem, case))
			})
			.unzip();
		let (subcmd_enumitems, subcmd_cases): (Vec<_>, Vec<_>) =
			subcmd_patterns.into_iter().unzip();

		let subcmd_field = if !subcmd_enumitems.is_empty() {
			quote! {
				#[clap(subcommand)]
				__fncmd_subcmds: __fncmd_subcmds,
			}
		} else {
			quote! {}
		};

		let subcmd_enum = if !subcmd_enumitems.is_empty() {
			quote! {
				#[derive(fncmd::clap::Parser)]
				#visibility enum __fncmd_subcmds {
					#(#subcmd_enumitems,)*
				}
			}
		} else {
			quote! {}
		};

		let exec_impl = if !subcmd_cases.is_empty() {
			quote! {
				let __fncmd_options {
					#(#vars,)*
					__fncmd_subcmds,
					..
				} = __fncmd_options.unwrap_or_else(|| {
					use fncmd::clap::Parser;
					__fncmd_options::parse()
				});
				match __fncmd_subcmds {
					#(#subcmd_cases)*
					_ => {
						#body
					}
				}
			}
		} else {
			quote! {
				let __fncmd_options {
					#(#vars,)*
					..
				} = __fncmd_options.unwrap_or_else(|| {
					use fncmd::clap::Parser;
					__fncmd_options::parse()
				});
				#body
			}
		};

		let code = quote! {
		  use fncmd::clap;
			#(#subcmd_imports)*

			#[doc(hidden)]
			#[allow(non_camel_case_types)]
			#(#attrs)*
			#doc
			#[derive(fncmd::clap::Parser)]
			#[clap(name = #cmd_name, version = #version)]
			#visibility struct __fncmd_options {
				#(#args,)*
				#subcmd_field
			}

			#subcmd_enum

			fn __fncmd_exec_impl(__fncmd_options: Option<__fncmd_options>) #return_type {
				#exec_impl
			}

			#[inline]
			#visibility fn __fncmd_exec(__fncmd_options: Option<__fncmd_options>) -> fncmd::Result {
				__fncmd_exec_impl(__fncmd_options).into()
			}

			fn main() #return_type {
				__fncmd_exec(None).into()
			}
		};

		code.to_tokens(tokens);
	}
}