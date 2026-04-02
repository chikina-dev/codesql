#!/usr/bin/env bash

set -e

echo "======================================"
echo "    codesql Performance Benchmark     "
echo "======================================"
echo ""

echo "Building codesql (release) - Please wait..."
cargo build --release --quiet
CODESQL="./target/release/codesql"

QUERY_TEXT="struct"
SQL_TEXT="SELECT path FROM files WHERE contains(content, 'struct')"

QUERY_REGEX="struct.*\{"
SQL_REGEX="SELECT path FROM files WHERE regex(content, 'struct.*\{')"

QUERY_SYM="fn "
SQL_SYM="SELECT path FROM files WHERE has_symbol('Function', glob('*'))"

ms_time() {
    python3 -c 'import time; print(int(time.time()*1000))'
}

run_benchmark() {
    local label="$1"
    local iters="$2"
    local cmd="$3"

    local start=$(ms_time)
    for i in $(seq 1 "$iters"); do
        eval "$cmd" > /dev/null 2>&1 || true
    done
    local end=$(ms_time)

    local total=$((end - start))
    if [ "$iters" -eq 1 ]; then
        printf "%-25s | %-8s | %-15s\n" "$label" "${iters}回" "${total} ms"
    else
        local avg=$((total / iters))
        printf "%-25s | %-8s | %s\n" "$label" "${iters}回" "Total: ${total} ms (Avg: ${avg} ms)"
    fi
}

echo ""
echo "[1] 単体検証 - テキスト検索 (contains)"
echo "-----------------------------------------------------------------"
printf "%-25s | %-8s | %-15s\n" "コマンド" "実行回数" "所要時間"
echo "-----------------------------------------------------------------"
run_benchmark "grep -r" 1 "grep -rn '$QUERY_TEXT' src/ tests/"
run_benchmark "rg" 1 "rg '$QUERY_TEXT' src/ tests/"

rm -rf .codesql
$CODESQL init > /dev/null 2>&1
run_benchmark "codesql save" 1 "$CODESQL save"
run_benchmark "codesql search (text)" 1 "$CODESQL search \"$SQL_TEXT\""

echo ""
echo "[2] 単体検証 - 正規表現検索 (regex)"
echo "-----------------------------------------------------------------"
run_benchmark "grep -rE" 1 "grep -rnE '$QUERY_REGEX' src/ tests/"
run_benchmark "rg (regex)" 1 "rg '$QUERY_REGEX' src/ tests/"
run_benchmark "codesql search (regex)" 1 "$CODESQL search \"$SQL_REGEX\""

echo ""
echo "[3] 単体検証 - 意味的シンボル検索 (has_symbol)"
echo "※grep/rgは単純な文字列'fn 'で代用比較"
echo "-----------------------------------------------------------------"
run_benchmark "grep -r" 1 "grep -rn '$QUERY_SYM' src/ tests/"
run_benchmark "rg" 1 "rg '$QUERY_SYM' src/ tests/"
run_benchmark "codesql search (symbol)" 1 "$CODESQL search \"$SQL_SYM\""

echo ""
echo "[4] 複数回検証 (10回連続・テキスト検索)"
echo "-----------------------------------------------------------------"
printf "%-25s | %-8s | %-15s\n" "コマンド" "実行回数" "総所要時間"
echo "-----------------------------------------------------------------"
run_benchmark "grep -r" 10 "grep -rn '$QUERY_TEXT' src/ tests/"
run_benchmark "rg" 10 "rg '$QUERY_TEXT' src/ tests/"
run_benchmark "codesql search" 10 "$CODESQL search \"$SQL_TEXT\""

echo ""
echo "[5] フルパイプライン (Save 1回 + Search 10回)"
echo "-----------------------------------------------------------------"
rm -rf .codesql
$CODESQL init > /dev/null 2>&1
start_time=$(ms_time)
$CODESQL save > /dev/null 2>&1
for i in $(seq 1 10); do
    $CODESQL search "$SQL_TEXT" > /dev/null 2>&1 || true
done
end_time=$(ms_time)
total_time=$((end_time - start_time))
printf "%-25s | %-8s | %s\n" "codesql(+save/srch)" "1回+10回" "Total: ${total_time} ms"

echo "-----------------------------------------------------------------"
echo "Done."
