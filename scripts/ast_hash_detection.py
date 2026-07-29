import argparse
import json
from collections import Counter
from typing import Dict, Any, Iterator, Optional, Tuple

FNV_OFFSET_BASIS_64 = 14695981039346656037
FNV_PRIME_64 = 1099511628211
MASK_64 = 0xFFFFFFFFFFFFFFFF


def fnv1a_64_str(s: str) -> int:
    h = FNV_OFFSET_BASIS_64
    for b in s.encode("utf-8"):
        h ^= b
        h = (h * FNV_PRIME_64) & MASK_64
    return h


def hash64_to_hex(h: int) -> str:
    return f"{h:016x}"


def read_jsonl(path: str) -> Iterator[Dict[str, Any]]:
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            yield json.loads(line)


def compute_subtree_hash_from_dict(node: Dict[str, Any], hncd: Counter) -> int:
    node_type = str(node.get("node_type", ""))
    self_hash = fnv1a_64_str(node_type)

    children = node.get("children", [])
    if not isinstance(children, list) or not children:
        node_hash = self_hash
        hncd[hash64_to_hex(node_hash)] += 1
        return node_hash

    children_hash_sum = 0
    for child in children:
        if isinstance(child, dict):
            child_hash = compute_subtree_hash_from_dict(child, hncd)
            children_hash_sum = (children_hash_sum + child_hash) & MASK_64

    node_hash = (self_hash + children_hash_sum) & MASK_64
    hncd[hash64_to_hex(node_hash)] += 1
    return node_hash


def build_hash_tree_from_dict(ast_dict: Dict[str, Any]) -> Counter:
    hncd = Counter()
    compute_subtree_hash_from_dict(ast_dict, hncd)
    return hncd


def get_hncd_cached(
    cache: Dict[str, Optional[Counter]],
    function_id: str,
    ast_dict: Any,
) -> Optional[Counter]:
    if function_id in cache:
        return cache[function_id]

    if not isinstance(ast_dict, dict):
        cache[function_id] = None
        return None

    hncd = build_hash_tree_from_dict(ast_dict)
    cache[function_id] = hncd
    return hncd


def at_similarity_scores_a2(hncd_a: Counter, hncd_b: Counter) -> Dict[str, Any]:
    common_nodes = 0
    smaller, larger = (hncd_a, hncd_b) if len(hncd_a) <= len(hncd_b) else (hncd_b, hncd_a)

    for k, va in smaller.items():
        vb = larger.get(k)
        if vb is not None:
            common_nodes += min(va, vb)

    total_a = sum(hncd_a.values())
    total_b = sum(hncd_b.values())
    max_size = max(total_a, total_b)

    at_score = (common_nodes / max_size) if max_size else 0.0
    at_dice = (2.0 * common_nodes / (total_a + total_b)) if (total_a + total_b) else 0.0

    return {
        "AT": at_score,
        "AT_dice": at_dice,
        "common_nodes": common_nodes,
        "ast_size_a": total_a,
        "ast_size_b": total_b,
    }


def build_output_record(
    pair: Dict[str, Any],
    scores: Dict[str, Any],
    threshold: float,
    dice_threshold: float,
    pass_reason: str,
) -> Dict[str, Any]:
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
        "AT": scores["AT"],
        "AT_dice": scores["AT_dice"],
        "common_nodes": scores["common_nodes"],
        "ast_size_a": scores["ast_size_a"],
        "ast_size_b": scores["ast_size_b"],
        "a2_threshold": threshold,
        "a2_dice_threshold": dice_threshold,
        "pass_a2_reason": pass_reason,
        "pass_a2": True,
    }


def process_file(
    in_path: str,
    out_path: str,
    threshold: float = 0.65,
    dice_threshold: float = 0.70,
    progress_every: int = 5000,
) -> None:
    hncd_cache: Dict[str, Optional[Counter]] = {}
    total_pairs = 0
    valid_ast_pairs = 0
    pass_pairs = 0
    pass_by_original = 0
    pass_by_dice_only = 0
    missing_ast = 0

    with open(out_path, "w", encoding="utf-8") as out:
        for pair in read_jsonl(in_path):
            total_pairs += 1
            fa_id = str(pair.get("function_a_id", ""))
            fb_id = str(pair.get("function_b_id", ""))

            hncd_a = get_hncd_cached(hncd_cache, fa_id, pair.get("function_a_raw_ast"))
            hncd_b = get_hncd_cached(hncd_cache, fb_id, pair.get("function_b_raw_ast"))

            if hncd_a is None or hncd_b is None:
                missing_ast += 1
                continue

            valid_ast_pairs += 1
            scores = at_similarity_scores_a2(hncd_a, hncd_b)
            at_score = scores["AT"]
            at_dice = scores["AT_dice"]

            pass_original = at_score >= threshold
            pass_dice = at_dice >= dice_threshold

            if pass_original or pass_dice:
                pass_pairs += 1
                if pass_original:
                    pass_by_original += 1
                    reason = "AT>=threshold"
                else:
                    pass_by_dice_only += 1
                    reason = "AT_dice>=dice_threshold"

                out.write(json.dumps(build_output_record(pair, scores, threshold, dice_threshold, reason), ensure_ascii=False))
                out.write("\n")

            if progress_every > 0 and (total_pairs == 1 or total_pairs % progress_every == 0):
                print(
                    f"[A2-CACHED] processed={total_pairs} valid={valid_ast_pairs} pass={pass_pairs} "
                    f"cache={len(hncd_cache)} missing_ast={missing_ast}",
                    flush=True,
                )

    print(f"[A2-CACHED] total_pairs={total_pairs}")
    print(f"[A2-CACHED] valid_ast_pairs={valid_ast_pairs}")
    print(f"[A2-CACHED] pass_pairs={pass_pairs}")
    print(f"[A2-CACHED] pass_by_original={pass_by_original}")
    print(f"[A2-CACHED] pass_by_dice_only={pass_by_dice_only}")
    print(f"[A2-CACHED] missing_ast={missing_ast}")
    print(f"[A2-CACHED] hncd_cache_size={len(hncd_cache)}")
    print(f"[A2-CACHED] threshold={threshold}")
    print(f"[A2-CACHED] dice_threshold={dice_threshold}")
    print(f"[A2-CACHED] output={out_path}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("input_jsonl")
    ap.add_argument("output_jsonl")
    ap.add_argument("mode", nargs="?", default="pass", choices=["pass", "all"])
    ap.add_argument("--threshold", type=float, default=0.65)
    ap.add_argument("--dice-threshold", type=float, default=0.70)
    ap.add_argument("--progress-every", type=int, default=5000)
    args = ap.parse_args()

    process_file(
        args.input_jsonl,
        args.output_jsonl,
        threshold=args.threshold,
        dice_threshold=args.dice_threshold,
        progress_every=args.progress_every,
    )
    print(f"\n处理完成，输出保存到: {args.output_jsonl}")
