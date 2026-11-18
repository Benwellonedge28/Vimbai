set -e
cd /tmp/vimbai
mkdir -p /tmp/trendstore
rm -f /tmp/trendstore/store.json
VIMBAI_STORE_FILE=/tmp/trendstore/store.json ./rust/target/debug/vimbai-api  > /tmp/trend_engine.log 2>&1 &
for i in $(seq 1 60); do curl -sf http://127.0.0.1:8099/health >/dev/null && break; sleep 0.5; done
B=http://127.0.0.1:8099
R=$(curl -s -X POST $B/identity/users/register -H 'Content-Type: application/json' -d '{"username":"trendz","email":"trendz@example.com","password":"CorrectHorse1!"}')
USERID=$(echo "$R" | python3 -c "import json,sys;print(json.load(sys.stdin).get('id',''))")
L=$(curl -s -X POST $B/identity/users/login -H 'Content-Type: application/json' -d '{"username":"trendz","password":"CorrectHorse1!"}')
TOKEN=$(echo "$L" | python3 -c "import json,sys;print(json.load(sys.stdin).get('access_token',''))")
if [ -z "$USERID" ]; then USERID=$(python3 - "$TOKEN" <<'PY2'
import base64,json,sys
p=sys.argv[1].split('.')[1]; p+='='*(-len(p)%4)
print(json.loads(base64.urlsafe_b64decode(p)).get('sub',''))
PY2
); fi
echo "token len ${#TOKEN} user $USERID"
H=(-H "Authorization: Bearer $TOKEN" -H "X-User-Id: $USERID" -H 'Content-Type: application/json')
BK=$(curl -s -X POST $B/books "${H[@]}" -d '{"name":"Trend Shop","tier":"business"}')
BOOK=$(echo "$BK" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('id') or d.get('book',{}).get('id',''))")
echo "book $BOOK"
BH=(-H "Authorization: Bearer $TOKEN" -H "X-User-Id: $USERID" -H "X-Book-Id: $BOOK" -H 'Content-Type: application/json')
curl -s -X POST $B/accounting/journal-entries "${BH[@]}" -d '{"entry_date":"2026-11-10","description":"sale","reference_number":"T-1","lines":[{"account_number":"1200","debit":"500.00","credit":"0"},{"account_number":"4000","debit":"0","credit":"500.00"}]}' -o /tmp/je.json -w "post entry: %{http_code}\n"
JE=$(python3 -c "import json;print(json.load(open('/tmp/je.json'))['id'])")
head -c 120 /tmp/je.json; echo
curl -s -X POST "$B/accounting/journal-entries/${JE}/post" "${BH[@]}" -o /tmp/post.json -w "post draft: %{http_code}\n"
echo "--- November"
curl -s "$B/accounting/income-statement?start_date=2026-11-01&end_date=2026-11-30" "${BH[@]}" | python3 -c "
import json,sys
d=json.load(sys.stdin)
print('net_income', d.get('net_income'))
print('revenues', [(r.get('category'), r.get('amount')) for r in d.get('revenues',[])])
print('expenses', [(r.get('category'), r.get('amount')) for r in d.get('expenses',[])])"
echo "--- October (quiet, not an error)"
curl -s -o /tmp/oct.json -w "oct: %{http_code}\n" "$B/accounting/income-statement?start_date=2026-10-01&end_date=2026-10-31" "${BH[@]}"
python3 -c "
import json
d=json.load(open('/tmp/oct.json'))
print('net_income', d.get('net_income'), '| revenues', d.get('revenues'), '| expenses', d.get('expenses'))"
echo "--- December slice of the same entry (out of period, must be zero)"
curl -s "$B/accounting/income-statement?start_date=2026-12-01&end_date=2026-12-31" "${BH[@]}" | python3 -c "
import json,sys; d=json.load(sys.stdin); print('net_income', d.get('net_income'))"
