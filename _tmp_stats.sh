cd /home/debian/Fedi3
TOK=$(sed -n 's/^FEDI3_RELAY_ADMIN_TOKEN=//p' .env | head -n1)
curl -sS -H "Authorization: Bearer $TOK" https://relay.fedi3.com/_fedi3/relay/stats | python3 -m json.tool