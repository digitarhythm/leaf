#!/bin/bash

# Leaf Deployment Script
# サーバー上の .env を保護しつつ、フロントエンドとバックエンドを同期します。

SERVER="157.7.198.188"
PORT="22001"
USER="kow"
REMOTE_PATH="/var/wwws/digitarhythm.net/leaf"

echo "🚀 Starting deployment to $SERVER..."

# 1. Frontend Build
echo "📦 Building frontend..."
trunk build --release --public-url /
if [ $? -ne 0 ]; then
    echo "❌ Build failed. Deployment aborted."
    exit 1
fi

# 2. Sync Frontend Assets (Excluding server configs and dependencies)
echo "🚚 Syncing frontend assets..."
rsync -avz --delete \
    --exclude '.env' \
    --exclude 'server/' \
    --exclude 'node_modules/' \
    -e "ssh -p $PORT" \
    dist/ $USER@$SERVER:$REMOTE_PATH
if [ $? -ne 0 ]; then
    echo "❌ Frontend sync failed."
    exit 1
fi

# 3. Sync Backend Code (Excluding dependencies and env)
echo "🚚 Syncing backend code..."
rsync -avz \
    --exclude 'node_modules/' \
    --exclude '.env' \
    --exclude '.git/' \
    -e "ssh -p $PORT" \
    server/ $USER@$SERVER:$REMOTE_PATH/server
if [ $? -ne 0 ]; then
    echo "❌ Backend sync failed."
    exit 1
fi

# 4. Restart Backend via PM2
echo "🔄 Restarting backend service..."
ssh -p $PORT $USER@$SERVER "cd $REMOTE_PATH/server && pm2 startOrRestart ecosystem.config.js --update-env"
if [ $? -ne 0 ]; then
    echo "❌ PM2 restart failed."
    exit 1
fi

echo "✅ Deployment successful!"
say "デプロイが正常に完了しました。"
