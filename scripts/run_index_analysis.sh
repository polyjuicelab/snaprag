#!/bin/bash
# Index Analysis Runner Script
# This script helps analyze database indexes using the SQL script

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}SnapRAG Index Analysis Tool${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Try to get DATABASE_URL from various sources
DATABASE_URL=""

# 1. Check environment variable
if [ -n "$DATABASE_URL" ]; then
    echo -e "${GREEN}✅ Using DATABASE_URL from environment${NC}"
elif [ -f ".env" ]; then
    echo -e "${YELLOW}📄 Found .env file, attempting to read DATABASE_URL...${NC}"
    DATABASE_URL=$(grep "^DATABASE_URL=" .env | cut -d'=' -f2- | tr -d '"' | tr -d "'" || echo "")
    if [ -z "$DATABASE_URL" ]; then
        echo -e "${YELLOW}   DATABASE_URL not found in .env, trying config.toml...${NC}"
    fi
fi

# 2. Check config.toml
if [ -z "$DATABASE_URL" ] && [ -f "config.toml" ]; then
    echo -e "${YELLOW}📄 Found config.toml, attempting to read database URL...${NC}"
    # Extract database URL from config.toml (format: url = "postgresql://...")
    DATABASE_URL=$(grep -A 5 "^\[database\]" config.toml | grep "url" | head -1 | sed 's/.*url.*=.*"\(.*\)".*/\1/' || echo "")
    if [ -z "$DATABASE_URL" ]; then
        echo -e "${YELLOW}   Database URL not found in config.toml...${NC}"
    fi
fi

# 3. Check if DATABASE_URL is provided as argument
if [ -z "$DATABASE_URL" ] && [ -n "$1" ]; then
    DATABASE_URL="$1"
    echo -e "${GREEN}✅ Using DATABASE_URL from command line argument${NC}"
fi

# Final check
if [ -z "$DATABASE_URL" ]; then
    echo -e "${RED}❌ Error: DATABASE_URL not found!${NC}"
    echo ""
    echo "Please provide DATABASE_URL in one of these ways:"
    echo "  1. Environment variable: export DATABASE_URL='postgresql://...'"
    echo "  2. .env file: DATABASE_URL='postgresql://...'"
    echo "  3. config.toml: [database] url = 'postgresql://...'"
    echo "  4. Command line: $0 'postgresql://...'"
    echo ""
    exit 1
fi

# Mask password in output
MASKED_URL=$(echo "$DATABASE_URL" | sed 's/:[^:@]*@/:***@/')
echo -e "${GREEN}✅ Database URL: ${MASKED_URL}${NC}"
echo ""

# Check if psql is available
if ! command -v psql &> /dev/null; then
    echo -e "${RED}❌ Error: psql command not found!${NC}"
    echo "Please install PostgreSQL client tools."
    exit 1
fi

# Check if analysis script exists
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANALYSIS_SCRIPT="$SCRIPT_DIR/analyze_indexes.sql"

if [ ! -f "$ANALYSIS_SCRIPT" ]; then
    echo -e "${RED}❌ Error: Analysis script not found: $ANALYSIS_SCRIPT${NC}"
    exit 1
fi

echo -e "${GREEN}📊 Running index analysis...${NC}"
echo ""

# Run the analysis
psql "$DATABASE_URL" -f "$ANALYSIS_SCRIPT" 2>&1 | tee index_analysis_output.txt

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Analysis Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "Results saved to: index_analysis_output.txt"
echo ""
echo "Next steps:"
echo "  1. Review the analysis output"
echo "  2. Check INDEX_OPTIMIZATION_PLAN.md for optimization recommendations"
echo "  3. Test new indexes in a development environment first"
echo ""

