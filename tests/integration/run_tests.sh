#!/bin/bash
# Integration tests for Roast examples
# Run with: bash tests/integration/run_tests.sh

set -e

ROASTC="${ROASTC:-./target/release/roastc}"
KITCHEN="${KITCHEN:-./target/release/kitchen}"

echo "🔥 Roast Integration Tests"
echo "=========================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

PASSED=0
FAILED=0

run_test() {
    local name="$1"
    local cmd="$2"
    
    echo -n "Testing $name... "
    if eval "$cmd" > /dev/null 2>&1; then
        echo -e "${GREEN}PASSED${NC}"
        ((PASSED++))
    else
        echo -e "${RED}FAILED${NC}"
        ((FAILED++))
    fi
}

# Test 1: Compile and run .ro file
run_test ".ro extension" "$ROASTC run tests/test_ro_extension.ro"

# Test 2: Compile and run .roast file
run_test ".roast extension" "$ROASTC run tests/test_minimal_class.roast"

# Test 3: Compile and run emoji extension
run_test "🍗 extension" "$ROASTC run 'tests/test_emoji.🍗'"

# Test 4: Todo app build
run_test "Todo app compile" "$ROASTC build examples/todo_api/src/main.roast -o /tmp/todo_test"

# Test 5: List operations
run_test "List operations" "$ROASTC run tests/test_list_append.roast"

# Test 6: Dict operations
run_test "Dict operations" "$ROASTC run tests/test_dict_ops.roast"

# Test 7: Kitchen build
run_test "Kitchen build" "cd examples/todo_api && $KITCHEN build"

echo ""
echo "=========================="
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed${NC}"
    exit 1
fi
