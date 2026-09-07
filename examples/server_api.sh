#!/usr/bin/env bash
# Example API usage for Kelp HTTP server
# Start server (no auth):
#   kelp serve ./example_task_db 0.0.0.0:7878
# Start server (with auth):
#   kelp serve ./example_task_db 0.0.0.0:7878 --auth-token mytoken

BASE="http://127.0.0.1:7878"
AUTH=""
# For token-protected server set AUTH header like:
# AUTH="-H 'Authorization: Bearer mytoken'"

# Create a schema
curl -s -X POST $BASE/schema \
  -H "Content-Type: application/json" \
  $AUTH \
  -d '{"name":"Task","fields":[{"name":"id","type":"String","required":true},{"name":"title","type":"String"},{"name":"status","type":"String"}]}'

echo

# Create an object
curl -s -X POST $BASE/put \
  -H "Content-Type: application/json" \
  $AUTH \
  -d '{"type":"Task","id":"t1","fields":{"id":"t1","title":"Example","status":"pending"}}'

echo

# Query by filters
curl -s -X POST $BASE/query \
  -H "Content-Type: application/json" \
  $AUTH \
  -d '{"type":"Task","filters":{"status":"pending"}}'

echo

# Get single object
curl -s "$BASE/get?type=Task&id=t1" $AUTH
echo

# List objects
curl -s "$BASE/list?type=Task" $AUTH
echo
