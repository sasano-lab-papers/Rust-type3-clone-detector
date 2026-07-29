import argparse
import json
from collections import Counter
from typing import Dict, Any, Iterator, Optional, Tuple


def read_jsonl(path: str) -> Iterator[Dict[str, Any]]:
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            yield json.loads(line)


def build_output_record(pair: Dict[str, Any], j_score: float, q: int, threshold: float) -> Dict[str, Any]:
    return {
        "function_a_id": pair.get("function_a_id"),
        "function_b_id": pair.get("function_b_id"),
        "sr": pair.get("sr"),
        "shared": pair.get("shared"),
        "max_len": pair.get("max_len"),
        "overlap_ratio": pair.get("overlap_ratio"),
        "start_line_a": pair.get("start_line_a"),
        "end_line_a": pair.get("end_line_a"),
        "start_line_b": pair.get("start_line_b"),
        "end_line_b": pair.get("end_line_b"),
        "J": j_score,
        "q": q,
        "a4_threshold": threshold,
        "pass_a4": True,
    }


def node_type(node: Dict[str, Any]) -> str:
    return str(node.get("node_type", ""))


def node_children(node: Dict[str, Any]):
    children = node.get("children", [])
    if isinstance(children, list):
        return [c for c in children if isinstance(c, dict)]
    return []


def build_atomic_pattern_dict(node: Dict[str, Any], q: int) -> str:
    if q <= 1:
        return node_type(node)
    children = node_children(node)
    if not children:
        return node_type(node)
    child_patterns = [build_atomic_pattern_dict(child, q - 1) for child in children]
    return f"{node_type(node)}(" + ",".join(child_patterns) + ")"


def compute_characteristic_vector_dict(node: Dict[str, Any], q: int) -> Counter:
    vector = Counter()
    for child in node_children(node):
        vector.update(compute_characteristic_vector_dict(child, q))
    vector[build_atomic_pattern_dict(node, q)] += 1
    return vector


def get_cv_cached(
    cache: Dict[Tuple[str, int], Optional[Counter]],
    function_id: str,
    ast_dict: Any,
    q: int,
) -> Optional[Counter]:
    key = (function_id, q)
    if key in cache:
        return cache[key]
    if not isinstance(ast_dict, dict):
        cache[key] = None
        return None
    cv = compute_characteristic_vector_dict(ast_dict, q)
    cache[key] = cv
    return cv


def jaccard_similarity(counter_a: Counter, counter_b: Counter) -> float:
    keys = set(counter_a.keys()) | set(counter_b.keys())
    intersection = sum(min(counter_a.get(k, 0), counter_b.get(k, 0)) for k in keys)
    union = sum(max(counter_a.get(k, 0), counter_b.get(k, 0)) for k in keys)
    return (intersection / union) if union else 0.0


def process_file(in_path: str, out_path: str, q: int = 1, threshold: float = 0.75, progress_every: int = 5000) -> None:
    cv_cache: Dict[Tuple[str, int], Optional[Counter]] = {}
    total_pairs = 0
    valid_ast_pairs = 0
    pass_pairs = 0
    missing_ast = 0

    with open(out_path, "w", encoding="utf-8") as out:
        for pair in read_jsonl(in_path):
            total_pairs += 1
            fa_id = str(pair.get("function_a_id", ""))
            fb_id = str(pair.get("function_b_id", ""))

            cv_a = get_cv_cached(cv_cache, fa_id, pair.get("function_a_raw_ast"), q)
            cv_b = get_cv_cached(cv_cache, fb_id, pair.get("function_b_raw_ast"), q)
            if cv_a is None or cv_b is None:
                missing_ast += 1
                continue

            valid_ast_pairs += 1
            j_score = jaccard_similarity(cv_a, cv_b)
            if j_score >= threshold:
                pass_pairs += 1
                out.write(json.dumps(build_output_record(pair, j_score, q, threshold), ensure_ascii=False))
                out.write("\n")

            if progress_every > 0 and (total_pairs == 1 or total_pairs % progress_every == 0):
                print(
                    f"[A4-CACHED] processed={total_pairs} valid={valid_ast_pairs} pass={pass_pairs} "
                    f"cache={len(cv_cache)} missing_ast={missing_ast}",
                    flush=True,
                )

    print(f"[A4-CACHED] total_pairs={total_pairs}")
    print(f"[A4-CACHED] valid_ast_pairs={valid_ast_pairs}")
    print(f"[A4-CACHED] pass_pairs={pass_pairs}")
    print(f"[A4-CACHED] missing_ast={missing_ast}")
    print(f"[A4-CACHED] cv_cache_size={len(cv_cache)}")
    print(f"[A4-CACHED] q={q}")
    print(f"[A4-CACHED] threshold={threshold}")
    print(f"[A4-CACHED] output={out_path}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("input_jsonl")
    ap.add_argument("output_jsonl")
    ap.add_argument("mode", nargs="?", default="pass", choices=["pass", "all"])
    ap.add_argument("--q", type=int, default=1)
    ap.add_argument("--threshold", type=float, default=0.75)
    ap.add_argument("--progress-every", type=int, default=5000)
    args = ap.parse_args()

    process_file(args.input_jsonl, args.output_jsonl, q=args.q, threshold=args.threshold, progress_every=args.progress_every)
    print(f"处理完成，输出保存到: {args.output_jsonl}")
