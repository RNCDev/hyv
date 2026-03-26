#!/bin/bash
set -e

echo "=== Hyv Setup ==="

# Check Python 3.10+
if ! command -v python3 &>/dev/null; then
    echo "Error: python3 not found. Install Python 3.10+ first."
    exit 1
fi

PY_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
PY_MAJOR=$(echo "$PY_VERSION" | cut -d. -f1)
PY_MINOR=$(echo "$PY_VERSION" | cut -d. -f2)

if [ "$PY_MAJOR" -lt 3 ] || { [ "$PY_MAJOR" -eq 3 ] && [ "$PY_MINOR" -lt 10 ]; }; then
    echo "Error: Python 3.10+ required, found $PY_VERSION"
    exit 1
fi
echo "✓ Python $PY_VERSION"

# Install dependencies
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
echo "Installing Python dependencies..."
pip3 install -r "$SCRIPT_DIR/requirements.txt"
echo "✓ Python dependencies installed"

# Check .env
ENV_FILE="$SCRIPT_DIR/../.env"
if [ ! -f "$ENV_FILE" ]; then
    echo ""
    echo "Creating .env template..."
    cat > "$ENV_FILE" << 'EOF'
COHERE_TRIAL_API_KEY=your-cohere-key-here
HF_TOKEN=your-huggingface-token-here
EOF
    echo "✓ Created .env — fill in your API keys"
else
    echo "✓ .env exists"
fi

# Check XcodeGen
if command -v xcodegen &>/dev/null; then
    echo "✓ XcodeGen found"
    echo "Generating Xcode project..."
    cd "$SCRIPT_DIR/.."
    xcodegen generate
    echo "✓ Hyv.xcodeproj generated"
else
    echo "⚠ XcodeGen not found. Install with: brew install xcodegen"
fi

echo ""
echo "=== Setup complete ==="
echo "Next: open Hyv.xcodeproj and build (⌘B)"
