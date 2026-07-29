import json
import math
import os
import sys
from collections import defaultdict
from itertools import combinations


MIN_TOKENS = 50
N = 3
THETA1 = 0.15  # SR 过滤阈值
THETA2 = 0.5  # SourcererCC overlap 
THETA3 = 0.7  # SR 直接认克隆阈值
THETA4 = 0.7  # overlap 直接认克隆阈值


def duquhanshu(path):
    funcs = {}
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            obj.setdefault("func_id", obj.get("func_id", "<unknown>"))
            obj.setdefault("normalized_lines", obj.get("normalized_lines", []))
            obj.setdefault("tokens", obj.get("tokens", []))

            if len(obj["tokens"]) <= MIN_TOKENS:
                continue

            funcs[obj["func_id"]] = obj
            
    return funcs


def nlines_and_index(funcs, N=3):
    nlines_per_func = {}
    index = defaultdict(set)

    for f in funcs.values():  
        fid = f["func_id"]
        lines = f["normalized_lines"]
        blocks = []

        if len(lines) < N:
            nlines_per_func[fid] = []
            continue
        
        for i in range(len(lines) - N + 1):
            block = "\n".join(lines[i:i+N])
            blocks.append(block)
            h = hash(block)
            index[h].add(fid)

        nlines_per_func[fid] = blocks
    return nlines_per_func, index


def compute_sr(funcs, nlines_per_func, index, N=3):
    func_ids = [f["func_id"] for f in funcs.values()]
    total_blocks = {fid: len(nlines_per_func.get(fid, [])) for fid in func_ids}
    shared_counts = defaultdict(int)
    sr_map = {}
    candidates_sr_pass = []

    for h, fids in index.items():
        if len(fids) < 2:
            continue
        
        for a, b in combinations(sorted(fids), 2):
            shared_counts[(a, b)] += 1

    for (a, b), shared in shared_counts.items():
        max_blocks = max(total_blocks.get(a, 0), total_blocks.get(b, 0))
        if max_blocks == 0:
            continue
        sr = shared / max_blocks
        sr_map[(a, b)] = sr
        if sr >= THETA1:
            candidates_sr_pass.append((a, b))
    return sr_map, candidates_sr_pass


def jianlisorted_tokens(funcs):
    sorted_tokens = {}

    for f in funcs.values():
        fid = f["func_id"]
        toks = list(f.get("tokens", []))
        toks = [str(t) for t in toks]
        toks.sort()
        sorted_tokens[fid] = toks
    return sorted_tokens


def shared_tokens(sorted_toks_a, sorted_toks_b):
    i = j = 0
    shared = 0
    len_a = len(sorted_toks_a)
    len_b = len(sorted_toks_b)
    while i < len_a and j < len_b:
        if sorted_toks_a[i] == sorted_toks_b[j]:
            shared += 1
            i += 1
            j += 1
        elif sorted_toks_a[i] < sorted_toks_b[j]:
            i += 1
        else:
            j += 1
    return shared, len_a, len_b


def sourcerercc_pass(sorted_toks_a, sorted_toks_b, THETA):
    shared, len_a, len_b = shared_tokens(sorted_toks_a, sorted_toks_b)
    max_len = max(len_a, len_b)
    required = math.ceil(THETA * max_len)
    return shared >= required, shared, max_len


def token_filter(funcs, sr_map, candidates_sr_pass):
    sorted_tokens_dict = jianlisorted_tokens(funcs)
    direct_clones = []
    ast_candidates = []
    all_token_candidates = []

    for a, b in candidates_sr_pass:
        sr = sr_map.get((a, b), 0.0)
        passed_theta2, shared2, maxlen2 = sourcerercc_pass(sorted_tokens_dict[a], sorted_tokens_dict[b], THETA2)
        overlap_ratio = (shared2 / maxlen2) if maxlen2 > 0 else 0.0
        passed_theta4, shared4, maxlen4 = sourcerercc_pass(sorted_tokens_dict[a], sorted_tokens_dict[b], THETA4)

        if sr >= THETA3 or passed_theta4:
            info = {
                "pair": (a, b),
                "sr": sr,
                "shared": shared4,
                "max_len": maxlen4,
                "overlap_ratio": overlap_ratio,
                "reason": "SR>=θ3 or overlap>=θ4 "
            }
            direct_clones.append(info)
            all_token_candidates.append(info)
            continue

        if passed_theta2:
            info = {
                "pair": (a, b),
                "sr": sr,
                "shared": shared2,
                "max_len": maxlen2,
                "overlap_ratio": overlap_ratio
            }
            ast_candidates.append(info)
            all_token_candidates.append(info)

    return direct_clones, ast_candidates, all_token_candidates


def output_to_txt(direct_clones, ast_candidates, funcs):
    with open('direct_clones.txt', 'w', encoding='utf-8') as f:
        f.write('已确认克隆对:\n')
        for info in direct_clones:
            a, b = info["pair"]
            start_line_a = funcs[a]["start_line"]
            end_line_a = funcs[a]["end_line"]
            start_line_b = funcs[b]["start_line"]
            end_line_b = funcs[b]["end_line"]
            f.write(f"{a} <-> {b}, SR={info['sr']:.3f}, shared={info['shared']}, max_len={info['max_len']}, overlap_ratio={info['overlap_ratio']:.3f}, reason={info['reason']}, start_line_a={start_line_a}, end_line_a={end_line_a}, start_line_b={start_line_b}, end_line_b={end_line_b}\n")

    with open('ast_candidates.txt', 'w', encoding='utf-8') as f:
        f.write('待 AST 分析的克隆对:\n')
        for info in ast_candidates:
            a, b = info["pair"]
            start_line_a = funcs[a]["start_line"]
            end_line_a = funcs[a]["end_line"]
            start_line_b = funcs[b]["start_line"]
            end_line_b = funcs[b]["end_line"]
            f.write(f"{a} <-> {b}, SR={info['sr']:.3f}, shared={info['shared']}, max_len={info['max_len']}, overlap_ratio={info['overlap_ratio']:.3f}, start_line_a={start_line_a}, end_line_a={end_line_a}, start_line_b={start_line_b}, end_line_b={end_line_b}\n")


def output_direct_clones_jsonl(direct_clones, funcs, output_path="direct_clones.jsonl"):
    with open(output_path, "w", encoding="utf-8") as f:
        for info in direct_clones:
            a, b = info["pair"]

            record = {
                "function_a_id": a,
                "function_b_id": b,
                "sr": info["sr"],
                "shared": info["shared"],
                "max_len": info["max_len"],
                "overlap_ratio": info["overlap_ratio"],
                "start_line_a": funcs[a]["start_line"],
                "end_line_a": funcs[a]["end_line"],
                "start_line_b": funcs[b]["start_line"],
                "end_line_b": funcs[b]["end_line"],
                "reason": info.get("reason", "")
            }

            json.dump(record, f, ensure_ascii=False)
            f.write("\n")


def output_ast_candidates_jsonl(ast_candidates, funcs, output_path="ast_candidates.jsonl"):
    with open(output_path, "w", encoding="utf-8") as f:
        for info in ast_candidates:
            a, b = info["pair"]

            record = {
                "function_a_id": a,
                "function_b_id": b,
                "sr": info["sr"],
                "shared": info["shared"],
                "max_len": info["max_len"],
                "overlap_ratio": info["overlap_ratio"],
                "start_line_a": funcs[a]["start_line"],
                "end_line_a": funcs[a]["end_line"],
                "start_line_b": funcs[b]["start_line"],
                "end_line_b": funcs[b]["end_line"],
            }

            json.dump(record, f, ensure_ascii=False)
            f.write("\n")


def main(jsonl_path, out_dir):
    os.makedirs(out_dir, exist_ok=True)

    funcs = duquhanshu(jsonl_path)
    print(f"共有函数个数: {len(funcs)}")

    for func_id, f in funcs.items():
        print(f"\n[Sample func] id={func_id}")
        print("  normalized_lines:", len(f.get("normalized_lines", [])))
        print("  tokens:", len(f.get("tokens", [])))
    
    nlines_per_func, index = nlines_and_index(funcs, N=N)
    print("\n[1] N-lines 统计：")

    for fid, blocks in list(nlines_per_func.items())[:20]:
        print(f"  {fid}: N-lines 数 = {len(blocks)}")
    
    sr_map, candidates_sr_pass = compute_sr(funcs, nlines_per_func, index, N=N)
    print("\n[2] 候选对及 SR（部分）：")

    for pair, sr in list(sr_map.items())[:20]:
        print(f"  {pair}: SR = {sr:.3f}")
    print("\n通过 SR>=θ1 的候选对：", candidates_sr_pass)
    
    direct_clones, ast_candidates, all_token_candidates = token_filter(funcs, sr_map, candidates_sr_pass)

    output_direct_clones_jsonl(
        direct_clones,
        funcs,
        os.path.join(out_dir, "direct_clones.jsonl")
    )
    output_ast_candidates_jsonl(
        ast_candidates,
        funcs,
        os.path.join(out_dir, "ast_candidates.jsonl")
    )
    output_ast_candidates_jsonl(
        all_token_candidates,
        funcs,
        os.path.join(out_dir, "all_token_candidates.jsonl")
    )
    print("\n[3] Token 阶段直接判定为克隆的对：")

    for info in direct_clones:
        a, b = info["pair"]
        print(f"  {a} <-> {b}, SR={info['sr']:.3f}, shared={info['shared']}, max_len={info['max_len']}, overlap_ratio={info['overlap_ratio']:.3f}, reason={info['reason']}")

    print("\n[4] 留给AST阶段的候选对：")
    for info in ast_candidates:
        a, b = info["pair"]
        print(f"  {a} <-> {b}, SR={info['sr']:.3f}, shared={info['shared']}, max_len={info['max_len']}, overlap_ratio={info['overlap_ratio']:.3f}")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("用法: python v1.3.py <functions.jsonl> <out_dir>")
        sys.exit(1)

    jsonl_path = sys.argv[1]
    out_dir = sys.argv[2]
    main(jsonl_path, out_dir)
