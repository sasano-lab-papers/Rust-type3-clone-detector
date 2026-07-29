use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{parse_str, ImplItemFn, ItemFn};

/// 这是从 ast_candidates.jsonl 里读进来的“输入记录”
/// 这里先只保留候选对本身的信息，不带 AST
#[derive(Debug, Serialize, Deserialize)]
struct CandidatePair {
    function_a_id: String,
    function_b_id: String,
    sr: f64,
    shared: usize,
    max_len: usize,
    overlap_ratio: f64,
    start_line_a: usize,
    end_line_a: usize,
    start_line_b: usize,
    end_line_b: usize,
}

/// 这是输出到 output.jsonl 的“结果记录”
/// 在候选对原始信息基础上，再补上两边函数的 AST
#[derive(Debug, Serialize, Deserialize)]
struct ClonePairWithAst {
    function_a_id: String,
    function_b_id: String,
    sr: f64,
    shared: usize,
    max_len: usize,
    overlap_ratio: f64,
    start_line_a: usize,
    end_line_a: usize,
    start_line_b: usize,
    end_line_b: usize,
    function_a_raw_ast: Option<AstNode>,
    function_b_raw_ast: Option<AstNode>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AstNode {
    node_type: String,
    children: Vec<AstNode>,
}

impl AstNode {
    fn new(node_type: impl Into<String>) -> Self {
        Self {
            node_type: node_type.into(),
            children: Vec::new(),
        }
    }
}

/// 用一个栈手动构建树
/// enter 时压栈，exit 时弹出并挂到父节点上
struct AstTreeBuilder {
    stack: Vec<AstNode>,
    root: Option<AstNode>,
}

impl AstTreeBuilder {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            root: None,
        }
    }

    fn enter(&mut self, node_type: impl Into<String>) {
        self.stack.push(AstNode::new(node_type));
    }

    fn exit(&mut self) {
        let node = self.stack.pop().expect("AST 构建栈下溢");
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(node);
        } else {
            self.root = Some(node);
        }
    }
}

/// 语句节点分类
fn stmt_kind(stmt: &syn::Stmt) -> &'static str {
    match stmt {
        syn::Stmt::Local(_) => "Stmt::Local",
        syn::Stmt::Item(_) => "Stmt::Item",
        syn::Stmt::Expr(_, semi) => {
            if semi.is_some() {
                "Stmt::Expr;"
            } else {
                "Stmt::Expr"
            }
        }
        syn::Stmt::Macro(_) => "Stmt::Macro",
    }
}
/// 表达式节点分类
fn expr_kind(expr: &syn::Expr) -> &'static str {
    match expr {
        syn::Expr::Array(_) => "Expr::Array",
        syn::Expr::Assign(_) => "Expr::Assign",
        syn::Expr::Async(_) => "Expr::Async",
        syn::Expr::Await(_) => "Expr::Await",
        syn::Expr::Binary(_) => "Expr::Binary",
        syn::Expr::Block(_) => "Expr::Block",
        syn::Expr::Break(_) => "Expr::Break",
        syn::Expr::Call(_) => "Expr::Call",
        syn::Expr::Cast(_) => "Expr::Cast",
        syn::Expr::Closure(_) => "Expr::Closure",
        syn::Expr::Const(_) => "Expr::Const",
        syn::Expr::Continue(_) => "Expr::Continue",
        syn::Expr::Field(_) => "Expr::Field",
        syn::Expr::ForLoop(_) => "Expr::ForLoop",
        syn::Expr::Group(_) => "Expr::Group",
        syn::Expr::If(_) => "Expr::If",
        syn::Expr::Index(_) => "Expr::Index",
        syn::Expr::Infer(_) => "Expr::Infer",
        syn::Expr::Let(_) => "Expr::Let",
        syn::Expr::Lit(_) => "Expr::Lit",
        syn::Expr::Loop(_) => "Expr::Loop",
        syn::Expr::Macro(_) => "Expr::Macro",
        syn::Expr::Match(_) => "Expr::Match",
        syn::Expr::MethodCall(_) => "Expr::MethodCall",
        syn::Expr::Paren(_) => "Expr::Paren",
        syn::Expr::Path(_) => "Expr::Path",
        syn::Expr::Range(_) => "Expr::Range",
        syn::Expr::Reference(_) => "Expr::Reference",
        syn::Expr::Repeat(_) => "Expr::Repeat",
        syn::Expr::Return(_) => "Expr::Return",
        syn::Expr::Struct(_) => "Expr::Struct",
        syn::Expr::Try(_) => "Expr::Try",
        syn::Expr::TryBlock(_) => "Expr::TryBlock",
        syn::Expr::Tuple(_) => "Expr::Tuple",
        syn::Expr::Unary(_) => "Expr::Unary",
        syn::Expr::Unsafe(_) => "Expr::Unsafe",
        syn::Expr::While(_) => "Expr::While",
        syn::Expr::Yield(_) => "Expr::Yield",
        _ => "Expr::Other",
    }
}

/// 这里先保持你原来的做法，不细分 Pat
fn pat_kind(_: &syn::Pat) -> &'static str {
    "Pat"
}

/// 这里也先保持你原来的做法，不细分 Type
fn type_kind(_: &syn::Type) -> &'static str {
    "Type"
}

impl<'ast> Visit<'ast> for AstTreeBuilder {
    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.enter("Block");
        visit::visit_block(self, node);
        self.exit();
    }

    fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
        self.enter(stmt_kind(node));
        visit::visit_stmt(self, node);
        self.exit();
    }

    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        self.enter(expr_kind(node));
        visit::visit_expr(self, node);
        self.exit();
    }

    fn visit_pat(&mut self, node: &'ast syn::Pat) {
        self.enter(pat_kind(node));
        visit::visit_pat(self, node);
        self.exit();
    }

    fn visit_type(&mut self, node: &'ast syn::Type) {
        self.enter(type_kind(node));
        visit::visit_type(self, node);
        self.exit();
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        self.enter("Path");
        visit::visit_path(self, node);
        self.exit();
    }

    fn visit_path_segment(&mut self, node: &'ast syn::PathSegment) {
        self.enter("PathSegment");
        visit::visit_path_segment(self, node);
        self.exit();
    }

    fn visit_path_arguments(&mut self, node: &'ast syn::PathArguments) {
        self.enter("PathArguments");
        visit::visit_path_arguments(self, node);
        self.exit();
    }

    fn visit_angle_bracketed_generic_arguments(
        &mut self,
        node: &'ast syn::AngleBracketedGenericArguments,
    ) {
        self.enter("AngleBracketedGenericArguments");
        visit::visit_angle_bracketed_generic_arguments(self, node);
        self.exit();
    }

    fn visit_parenthesized_generic_arguments(
        &mut self,
        node: &'ast syn::ParenthesizedGenericArguments,
    ) {
        self.enter("ParenthesizedGenericArguments");
        visit::visit_parenthesized_generic_arguments(self, node);
        self.exit();
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.enter("Arm");
        visit::visit_arm(self, node);
        self.exit();
    }

    fn visit_field_value(&mut self, node: &'ast syn::FieldValue) {
        self.enter("FieldValue");
        visit::visit_field_value(self, node);
        self.exit();
    }

    fn visit_member(&mut self, node: &'ast syn::Member) {
        self.enter("Member");
        visit::visit_member(self, node);
        self.exit();
    }

    fn visit_lit(&mut self, node: &'ast syn::Lit) {
        self.enter("Lit");
        visit::visit_lit(self, node);
        self.exit();
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.enter("Macro");
        visit::visit_macro(self, node);
        self.exit();
    }

    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        self.enter("Attribute");
        visit::visit_attribute(self, node);
        self.exit();
    }

    fn visit_generics(&mut self, node: &'ast syn::Generics) {
        self.enter("Generics");
        visit::visit_generics(self, node);
        self.exit();
    }

    fn visit_generic_argument(&mut self, node: &'ast syn::GenericArgument) {
        self.enter("GenericArgument");
        visit::visit_generic_argument(self, node);
        self.exit();
    }

    fn visit_where_clause(&mut self, node: &'ast syn::WhereClause) {
        self.enter("WhereClause");
        visit::visit_where_clause(self, node);
        self.exit();
    }

    fn visit_where_predicate(&mut self, node: &'ast syn::WherePredicate) {
        self.enter("WherePredicate");
        visit::visit_where_predicate(self, node);
        self.exit();
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        self.enter("Local");
        visit::visit_local(self, node);
        self.exit();
    }

    fn visit_local_init(&mut self, node: &'ast syn::LocalInit) {
        self.enter("LocalInit");
        visit::visit_local_init(self, node);
        self.exit();
    }

    fn visit_bin_op(&mut self, node: &'ast syn::BinOp) {
        self.enter("BinOp");
        visit::visit_bin_op(self, node);
        self.exit();
    }

    fn visit_un_op(&mut self, node: &'ast syn::UnOp) {
        self.enter("UnOp");
        visit::visit_un_op(self, node);
        self.exit();
    }

    fn visit_range_limits(&mut self, node: &'ast syn::RangeLimits) {
        self.enter("RangeLimits");
        visit::visit_range_limits(self, node);
        self.exit();
    }
}

/// 从函数体 block 构建简化 AST
fn build_ast_from_block(block: &syn::Block) -> Result<AstNode, String> {
    let mut builder = AstTreeBuilder::new();
    builder.visit_block(block);

    builder
        .root
        .ok_or_else(|| "AST 根节点为空".to_string())
}

/// 传入的是“完整函数源码”，解析后只取函数体 block 来建树。
/// v2: 先按普通 free function 解析；如果失败，再按 impl method 解析。
/// 这样可以兼容 `fn foo(&self, ...)` 这类方法片段。
fn parse_function_body_ast(code: &str) -> Result<AstNode, String> {
    if let Ok(item_fn) = parse_str::<ItemFn>(code) {
        return build_ast_from_block(&item_fn.block);
    }

    if let Ok(method_fn) = parse_str::<ImplItemFn>(code) {
        return build_ast_from_block(&method_fn.block);
    }

    Err("函数源码解析失败：既不能解析为 ItemFn，也不能解析为 ImplItemFn".to_string())
}

/// 按起止行号从源码里切出函数文本
fn extract_code_by_lines(code: &str, start_line: usize, end_line: usize) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let start_index = start_line.saturating_sub(1);
    let end_index = end_line.min(lines.len());

    if start_index >= end_index {
        return String::new();
    }

    lines[start_index..end_index].join("\n")
}

/// 从 func_id 中取出相对文件路径
/// 约定 func_id 形如：
/// 1) src/a.rs::foo
/// 2) src/a.rs::Type::bar
/// 所以这里取第一个 "::" 之前的部分即可
fn extract_relative_file_from_func_id(func_id: &str) -> Result<&str, String> {
    func_id
        .split_once("::")
        .map(|(file_part, _)| file_part)
        .ok_or_else(|| format!("func_id 格式不合法，无法提取文件路径: {}", func_id))
}

/// 根据 project_root + func_id 找到真实源码文件
fn resolve_source_path(project_root: &Path, func_id: &str) -> Result<PathBuf, String> {
    let relative_file = extract_relative_file_from_func_id(func_id)?;
    Ok(project_root.join(relative_file))
}

/// 读取 jsonl 候选对
fn read_candidate_pairs(
    candidate_path: &str,
) -> Result<Vec<CandidatePair>, Box<dyn std::error::Error>> {
    let file = File::open(candidate_path)?;
    let reader = BufReader::new(file);

    let mut pairs = Vec::new();

    for (line_no, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        let pair: CandidatePair = serde_json::from_str(line).map_err(|e| {
            format!("第 {} 行 JSON 解析失败: {} | 内容: {}", line_no + 1, e, line)
        })?;

        pairs.push(pair);
    }

    Ok(pairs)
}

/// 带缓存地读取源码文件，避免同一个文件反复打开
fn get_source_code<'a>(
    cache: &'a mut HashMap<PathBuf, String>,
    file_path: &Path,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    if !cache.contains_key(file_path) {
        let content = std::fs::read_to_string(file_path)?;
        cache.insert(file_path.to_path_buf(), content);
    }

    Ok(cache
        .get(file_path)
        .map(|s| s.as_str())
        .expect("源码缓存读取失败"))
}


/// 带缓存地提取并解析函数 AST。
/// 关键优化：同一个 function_id 在很多候选 pair 中会重复出现；
/// 原版每出现一次就 parse 一次，ast_candidates 多时会极慢。
/// 这里按 function_id + start/end line 缓存 AST，通常可以把 parse 次数从 2*候选对数降到函数数量级。
fn get_or_parse_function_ast(
    ast_cache: &mut HashMap<String, Option<AstNode>>,
    source_cache: &mut HashMap<PathBuf, String>,
    project_root: &Path,
    function_id: &str,
    start_line: usize,
    end_line: usize,
    side_name: &str,
) -> Option<AstNode> {
    let cache_key = format!("{}:{}:{}", function_id, start_line, end_line);
    if let Some(cached) = ast_cache.get(&cache_key) {
        return cached.clone();
    }

    let path = match resolve_source_path(project_root, function_id) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("函数 {} 路径解析失败 [{}]: {}", side_name, function_id, err);
            ast_cache.insert(cache_key, None);
            return None;
        }
    };

    let source = match get_source_code(source_cache, &path) {
        Ok(code) => code.to_string(),
        Err(err) => {
            eprintln!("读取函数 {} 源码文件失败 [{}]: {}", side_name, path.display(), err);
            ast_cache.insert(cache_key, None);
            return None;
        }
    };

    let function_code = extract_code_by_lines(&source, start_line, end_line);
    let ast = match parse_function_body_ast(&function_code) {
        Ok(ast) => Some(ast),
        Err(err) => {
            eprintln!("函数 {} AST 解析失败 [{}]: {}", side_name, function_id, err);
            None
        }
    };

    ast_cache.insert(cache_key, ast.clone());
    ast
}

fn parse_args() -> Result<(String, String, String), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!(
            "用法: {} <ast_candidates.jsonl> <project_root> <output.jsonl>",
            args[0]
        );
        eprintln!(
            "例如: {} ast_candidates.jsonl ./my_rust_project output.jsonl",
            args[0]
        );
        std::process::exit(1);
    }

    Ok((args[1].clone(), args[2].clone(), args[3].clone()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (candidate_path, project_root, output_path) = parse_args()?;
    let project_root = PathBuf::from(project_root);

    let input_file = File::open(&candidate_path)?;
    let reader = BufReader::new(input_file);

    let output_file = File::create(&output_path)?;
    let mut writer = BufWriter::new(output_file);

    // 源码缓存：key 是文件路径，value 是整个文件文本
    let mut source_cache: HashMap<PathBuf, String> = HashMap::new();

    // AST 缓存：key 是 function_id:start:end，value 是解析出的 AST 或 None
    let mut ast_cache: HashMap<String, Option<AstNode>> = HashMap::new();

    let mut total_pairs: usize = 0;
    let mut written_pairs: usize = 0;
    let mut json_errors: usize = 0;

    for (line_no, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(line) => line,
            Err(err) => {
                eprintln!("第 {} 行读取失败: {}", line_no + 1, err);
                continue;
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let pair: CandidatePair = match serde_json::from_str(line) {
            Ok(pair) => pair,
            Err(err) => {
                json_errors += 1;
                eprintln!("第 {} 行 JSON 解析失败: {} | 内容: {}", line_no + 1, err, line);
                continue;
            }
        };

        total_pairs += 1;
        if total_pairs == 1 || total_pairs % 1000 == 0 {
            eprintln!(
                "[AST] processed_pairs={} written={} ast_cache={} source_cache={} json_errors={}",
                total_pairs,
                written_pairs,
                ast_cache.len(),
                source_cache.len(),
                json_errors
            );
        }

        let function_a_raw_ast = get_or_parse_function_ast(
            &mut ast_cache,
            &mut source_cache,
            &project_root,
            &pair.function_a_id,
            pair.start_line_a,
            pair.end_line_a,
            "A",
        );

        let function_b_raw_ast = get_or_parse_function_ast(
            &mut ast_cache,
            &mut source_cache,
            &project_root,
            &pair.function_b_id,
            pair.start_line_b,
            pair.end_line_b,
            "B",
        );

        let out_record = ClonePairWithAst {
            function_a_id: pair.function_a_id,
            function_b_id: pair.function_b_id,
            sr: pair.sr,
            shared: pair.shared,
            max_len: pair.max_len,
            overlap_ratio: pair.overlap_ratio,
            start_line_a: pair.start_line_a,
            end_line_a: pair.end_line_a,
            start_line_b: pair.start_line_b,
            end_line_b: pair.end_line_b,
            function_a_raw_ast,
            function_b_raw_ast,
        };

        serde_json::to_writer(&mut writer, &out_record)?;
        writeln!(&mut writer)?;
        written_pairs += 1;
    }

    writer.flush()?;
    println!("输出已保存到 {}", output_path);
    println!(
        "[AST] done: processed_pairs={} written={} ast_cache={} source_cache={} json_errors={}",
        total_pairs,
        written_pairs,
        ast_cache.len(),
        source_cache.len(),
        json_errors
    );
    Ok(())
}
