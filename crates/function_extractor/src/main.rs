use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use quote::ToTokens;
use rustc_lexer::TokenKind;
use serde::Serialize;
use syn::spanned::Spanned;
use syn::{Item, ItemFn};
use walkdir::WalkDir;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize)]
struct FuncRecord {
    func_id: String,
    file: String,
    name: String,
    normalized_lines: Vec<String>,
    tokens: Vec<String>,
    start_line: usize,
    end_line: usize,
}

fn main() -> AnyResult<()> {
    let (project_root, output_path) = parse_args();

    let out_file = File::create(&output_path)?;
    let mut writer = BufWriter::new(out_file);

    process_project(&project_root, &mut writer)?;

    writer.flush()?;
    Ok(())
}



/// 解析命令行参数（第 0 个是程序自己名字。）
fn parse_args() -> (PathBuf, PathBuf) {
    let args: Vec<String> = env::args().collect();

    // 用法: <project_root> <output_jsonl>
    if args.len() != 3 {
        eprintln!("用法: {} <project_root> <output_jsonl>", args[0]);
        std::process::exit(1);
    }

    let project_root = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);

    (project_root, output_path)
}




/// 遍历项目中的所有 .rs 文件
fn process_project(project_root: &Path, writer: &mut BufWriter<File>) -> AnyResult<()> {
    for entry in WalkDir::new(project_root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if !is_rs_file(path) {
            continue;
        }

        process_source_file(project_root, path, writer)?;
    }

    Ok(())
}




/// 处理单个 Rust 源文件（把一个 .rs 文件读进来，解析成语法树，再把里面的顶层 item 一个个处理。）
fn process_source_file(
    project_root: &Path,
    file_path: &Path,
    writer: &mut BufWriter<File>,
) -> AnyResult<()> {
    let src = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读取失败: {}: {}", file_path.display(), e);
            return Ok(());
        }
    };



    // 语法分析使用syn库
    let syntax = match syn::parse_file(&src) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("解析失败: {}: {}", file_path.display(), err);
            return Ok(());
        }
    };

    for item in syntax.items {
        process_item(project_root, file_path, item, writer)?;
    }

    Ok(())
}



/// 处理 syn::File 里的一个 item
fn process_item(
    project_root: &Path,
    file_path: &Path,
    item: Item,
    writer: &mut BufWriter<File>,
) -> AnyResult<()> {
    match item {
        // 顶层 fn
        Item::Fn(free_fn) => {
            let rec = record_from_item_fn(project_root, file_path, &free_fn, None);
            write_record(writer, &rec)?;
        }

        // impl 内方法（trait impl の同名メソッドが func_id 上で衝突しないように impl 情報を作る。）
        Item::Impl(item_impl) => {
            let impl_ty = build_impl_label(&item_impl);

            for impl_item in item_impl.items {
                if let syn::ImplItem::Fn(method) = impl_item {
                    let tmp_fn = method_to_item_fn(&method);
                    let rec =
                        record_from_item_fn(project_root, file_path, &tmp_fn, Some(&impl_ty));
                    write_record(writer, &rec)?;
                }
            }
        }

        _ => {}
    }

    Ok(())
}

/// 把 impl 方法转成 ItemFn，复用同一套记录逻辑
fn method_to_item_fn(method: &syn::ImplItemFn) -> ItemFn {
    ItemFn {
        attrs: method.attrs.clone(),
        vis: method.vis.clone(),
        sig: method.sig.clone(),
        block: Box::new(method.block.clone()),
    }
}

/// 写一条 JSONL 记录
fn write_record(writer: &mut BufWriter<File>, rec: &FuncRecord) -> AnyResult<()> {
    serde_json::to_writer(&mut *writer, rec)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn is_rs_file(path: &Path) -> bool {
    path.is_file() && path.extension().map(|e| e == "rs").unwrap_or(false)
}

/// 只删除文档注释属性：
/// Rust 的 `/// xxx` 在 syn 里会变成 `#[doc = "xxx"]` 放在 attrs 里
/// 删除它不会影响函数体 AST，只会让输出的 normalized_lines / tokens 更干净
fn strip_doc_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|a| !a.path().is_ident("doc"));
}

/// 核心：把一个函数（顶层 fn 或 impl 方法）变成 FuncRecord
fn record_from_item_fn(
    project_root: &Path,
    file_path: &Path,
    item_fn: &ItemFn,
    impl_ty: Option<&str>,
) -> FuncRecord {
    let name = item_fn.sig.ident.to_string();

    // 行号：用 fn 关键字行 + 函数体 block 结束行
    let start_line = item_fn.sig.fn_token.span.start().line;
    let end_line = item_fn.block.span().end().line;

    let func_str = build_function_string(item_fn);
    let normalized_lines = normalize_lines(&func_str);
    let tokens = lex_and_normalize_tokens(&func_str);

    let file_str = build_file_str(project_root, file_path);
    let func_id = build_func_id(&file_str, impl_ty, &name);

    FuncRecord {
        func_id,
        file: file_str,
        name,
        normalized_lines,
        tokens,
        start_line,
        end_line,
    }
}

/// 生成“清洗过 doc 注释之后”的单函数字符串
fn build_function_string(item_fn: &ItemFn) -> String {
    let mut clean_fn = item_fn.clone();
    strip_doc_attrs(&mut clean_fn.attrs);

    let tmp_file = syn::File {
        shebang: None,
        attrs: vec![],
        items: vec![Item::Fn(clean_fn)],
    };

    prettyplease::unparse(&tmp_file)
}

/// 生成 TACC 风格的按行归一化输出：
/// - 删除空白和注释
/// - 字面量保留原文本
/// - lifetime 统一为 LIFETIME
/// - 标识符统一为 ID，但 Rust 关键字保留原样
///
/// 注意：这里按源码中的换行边界 flush，保证后面的 N-lines 仍然是“行序列”。
fn normalize_lines(src: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut offset = 0usize;

    for t in rustc_lexer::tokenize(src) {
        let len = t.len as usize;

        if offset + len > src.len() {
            break;
        }

        let text = &src[offset..offset + len];

        match t.kind {
            TokenKind::Whitespace => {
                if text.contains('\n') {
                    flush_token_line(&mut lines, &mut current);
                }
            }
            TokenKind::LineComment { .. } | TokenKind::BlockComment { .. } => {
                if text.contains('\n') {
                    flush_token_line(&mut lines, &mut current);
                }
            }
            _ => {
                if let Some(tok) = normalize_token(t.kind, text) {
                    current.push(tok);
                }
            }
        }

        offset += len;
    }

    flush_token_line(&mut lines, &mut current);
    lines
}

fn flush_token_line(lines: &mut Vec<String>, current: &mut Vec<String>) {
    if !current.is_empty() {
        lines.push(current.join(" "));
        current.clear();
    }
}

/// 生成相对文件路径字符串
fn build_file_str(project_root: &Path, file_path: &Path) -> String {
    file_path
        .strip_prefix(project_root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string()
}

/// impl 块的标识文字を生成する
/// - impl Type の場合: impl Type
/// - impl Trait for Type の場合: impl Trait for Type
///
/// trait impl の同名メソッドが func_id 上で衝突しないようにする。
fn build_impl_label(item_impl: &syn::ItemImpl) -> String {
    let self_ty = item_impl.self_ty.to_token_stream().to_string();

    match &item_impl.trait_ {
        Some((_, trait_path, _)) => {
            let trait_name = trait_path.to_token_stream().to_string();
            format!("impl {} for {}", trait_name, self_ty)
        }
        None => {
            format!("impl {}", self_ty)
        }
    }
}

/// 按原规则构造 func_id
fn build_func_id(file_str: &str, impl_ty: Option<&str>, name: &str) -> String {
    match impl_ty {
        Some(ty) => format!("{}::{}::{}", file_str, ty, name),
        None => format!("{}::{}", file_str, name),
    }
}

fn lex_and_normalize_tokens(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut offset = 0usize;

    for t in rustc_lexer::tokenize(src) {
        let len = t.len as usize;

        if offset + len > src.len() {
            break;
        }

        let text = &src[offset..offset + len];

        if let Some(tok) = normalize_token(t.kind, text) {
            out.push(tok);
        }

        offset += len;
    }

    out
}

/// 单个 token 的归一化规则：
/// - 空白 / 注释：删除
/// - 字面量：保留原文本
/// - lifetime：LIFETIME
/// - 标识符：Rust 关键字保留，普通标识符替换为 ID
/// - 其他符号 / 运算符：保留原文本
fn normalize_token(kind: TokenKind, text: &str) -> Option<String> {
    match kind {
        TokenKind::Whitespace => None,
        TokenKind::LineComment { .. } | TokenKind::BlockComment { .. } => None,
        TokenKind::Literal { .. } => Some(text.to_string()),
        TokenKind::Lifetime { .. } => Some("LIFETIME".to_string()),
        TokenKind::Ident | TokenKind::RawIdent => {
            if is_rust_keyword(text) {
                Some(text.to_string())
            } else {
                Some("ID".to_string())
            }
        }
        _ => Some(text.to_string()),
    }
}

fn is_rust_keyword(text: &str) -> bool {
    matches!(
        text,
        // strict keywords
        "as"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"

            // 2018 / async-related
            | "async"
            | "await"
            | "dyn"

            // reserved keywords
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "gen"

            // placeholder pattern
            | "_"
    )
}
