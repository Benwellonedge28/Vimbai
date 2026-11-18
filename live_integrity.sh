set -e
cd /tmp/vimbai
pkill -f vimbai-api >/dev/null 2>&1 || true
mkdir -p /tmp/intstore
rm -f /tmp/intstore/store.json
VIMBAI_STORE_FILE=/tmp/intstore/store.json ./rust/target/debug/vimbai-api > /tmp/int_engine.log 2>&1 &
for i in $(seq 1 60); do curl -sf http://127.0.0.1:8099/health >/dev/null && break; sleep 0.5; done
B=http://127.0.0.1:8099
R=$(curl -s -X POST $B/identity/users/register -H 'Content-Type: application/json' -d '{"username":"chain","email":"chain@example.com","password":"CorrectHorse1!"}')
USERID=$(echo "$R" | python3 -c "import json,sys;print(json.load(sys.stdin).get('id',''))")
TOKEN=$(curl -s -X POST $B/identity/users/login -H 'Content-Type: application/json' -d '{"username":"chain","password":"CorrectHorse1!"}' | python3 -c "import json,sys;print(json.load(sys.stdin).get('access_token',''))")
H=(-H "Authorization: Bearer $TOKEN" -H "X-User-Id: $USERID" -H 'Content-Type: application/json')
BOOK=$(curl -s -X POST $B/books "${H[@]}" -d '{"name":"Chain Shop","tier":"business"}' | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('id') or d.get('book',{}).get('id',''))")
BH=(-H "Authorization: Bearer $TOKEN" -H "X-User-Id: $USERID" -H "X-Book-Id: $BOOK" -H 'Content-Type: application/json')
echo "--- empty book first"
curl -s "$B/accounting/ledger/integrity" "${BH[@]}" | head -c 400; echo
for n in 1 2 3; do
  JE=$(curl -s -X POST $B/accounting/journal-entries "${BH[@]}" -d "{\"entry_date\":\"2026-11-1$n\",\"description\":\"sale $n\",\"reference_number\":\"C-$n\",\"lines\":[{\"account_number\":\"1200\",\"debit\":\"100.00\",\"credit\":\"0\"},{\"account_number\":\"4000\",\"debit\":\"0\",\"credit\":\"100.00\"}]}" | python3 -c "import json,sys;print(json.load(sys.stdin)['id'])")
  curl -s -o /dev/null -X POST "$B/accounting/journal-entries/${JE}/post" "${BH[@]}"
done
echo "--- after 3 posted entries"
curl -s "$B/accounting/ledger/integrity" "${BH[@]}" | python3 -m json.tool | head -20
