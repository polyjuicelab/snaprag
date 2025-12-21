#!/bin/bash
# Test script to check annual report API response

FID="${1:-460432}"
YEAR="${2:-2024}"
API_URL="${API_URL:-http://localhost:3000}"

echo "🧪 Testing Annual Report API"
echo "FID: $FID"
echo "Year: $YEAR"
echo ""

# Test with verbose output
echo "📡 Making request..."
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_URL/api/users/$FID/annual-report/$YEAR")
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo ""

if [ "$HTTP_CODE" = "200" ]; then
    echo "✅ Request successful!"
    echo ""
    echo "Response structure:"
    echo "$BODY" | jq -r '
      "Success: " + (.success | tostring),
      "Year: " + (.data.year | tostring),
      "",
      "User:",
      "  FID: " + (.data.user.fid | tostring),
      "  Username: " + (.data.user.username // "null" | tostring),
      "",
      "Activity:",
      "  Total Casts: " + (.data.activity.total_casts | tostring),
      "  Casts in Year: " + (.data.activity.total_casts_in_year | tostring),
      "",
      "Content Style:",
      "  Top Emojis: " + (.data.content_style.top_emojis | length | tostring) + " items",
      "  Top Words: " + (.data.content_style.top_words | length | tostring) + " items",
      "  Frames Used: " + (.data.content_style.frames_used | tostring)
    ' 2>/dev/null || echo "Failed to parse JSON"
    
    echo ""
    echo "Top 3 Emojis:"
    echo "$BODY" | jq -r '.data.content_style.top_emojis[0:3] | .[] | "  \(.emoji): \(.count)"' 2>/dev/null || echo "  None"
    
    echo ""
    echo "Top 5 Words:"
    echo "$BODY" | jq -r '.data.content_style.top_words[0:5] | .[] | "  \(.word): \(.count)"' 2>/dev/null || echo "  None"
    
elif [ "$HTTP_CODE" = "401" ]; then
    echo "❌ Authentication required"
    echo "   Please restart the server after setting auth.enabled = false in config.toml"
elif [ "$HTTP_CODE" = "404" ]; then
    echo "❌ User not found (FID: $FID)"
else
    echo "❌ Error: HTTP $HTTP_CODE"
    echo "Response:"
    echo "$BODY" | head -20
fi

