#!/bin/bash
# Docker-based Ergo development environment
# Use this if you don't have Ergo tools installed locally

set -e

# Configuration
CONTAINER_NAME="ergo-dev"
ERGO_VERSION="4.0.28"
VOLUME_MOUNT="/home/n1ur0/Xergon-Network:/app"

echo "=== Xergon Network - Docker Ergo Environment ==="

# Check if Docker is running
if ! docker info &> /dev/null; then
    echo "ERROR: Docker is not running"
    exit 1
fi

# Check if container exists
if docker ps -a | grep -q "$CONTAINER_NAME"; then
    echo "Container $CONTAINER_NAME exists"
    docker start "$CONTAINER_NAME"
else
    echo "Creating new container..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        -v "$VOLUME_MOUNT" \
        -w /app \
        ergoplatform/ergo:$ERGO_VERSION \
        tail -f /dev/null
fi

# Enter container
echo "Entering container..."
docker exec -it "$CONTAINER_NAME" /bin/bash

# Alternative: Run a single command
# docker exec -it "$CONTAINER_NAME" ergo-compiler compile contracts/governance_proposal_v2.ergo -o /app/contracts/governance_proposal_v2.ergotree
