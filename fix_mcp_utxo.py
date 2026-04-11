#!/usr/bin/env python3
"""
Fix MCP and Ergo Node Configuration
"""

import os
import json
import requests
import yaml

HERMES_HOME = os.path.expanduser("~/.hermes")
CONFIG_PATH = os.path.join(HERMES_HOME, "config.yaml")
ERGONODE_IP = "192.168.1.75"
ERGONODE_PORT = 9052

def fix_mcp_config():
    """Fix MCP server configuration"""
    print("=== Fixing MCP Configuration ===\n")
    
    # Load config
    with open(CONFIG_PATH, 'r') as f:
        config = yaml.safe_load(f)
    
    # Update MCP servers with correct endpoints
    mcp_servers = {
        "ergo-transcript": {
            "url": "https://ergo-transcripts.vercel.app/api/mcp"
        },
        "ergo-knowledge": {
            "url": "https://ergo-knowledge-base.vercel.app/api/mcp"
        }
    }
    
    # Test each MCP server
    for name, server in mcp_servers.items():
        print(f"Testing {name}...")
        url = server["url"]
        
        # Try different approaches
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream"
        }
        
        payload = {
            "jsonrpc": "2.0",
            "method": "list",
            "params": {},
            "id": 1
        }
        
        try:
            resp = requests.post(url, json=payload, headers=headers, timeout=10)
            print(f"  Status: {resp.status_code}")
            
            if resp.status_code == 200:
                print(f"  ✅ {name} is responding")
                result = resp.json()
                if "result" in result:
                    print(f"  Result: {result['result']}")
            elif resp.status_code == 406:
                print(f"  ⚠️  {name} requires specific headers")
                print(f"      Try: Accept: application/json, text/event-stream")
            elif resp.status_code == 405:
                print(f"  ⚠️  {name} doesn't support this method")
            else:
                print(f"  ❌ {name}: {resp.text[:100]}")
        except Exception as e:
            print(f"  ❌ {name}: {str(e)[:100]}")
        
        print()
    
    # Save config
    config["mcp_servers"] = mcp_servers
    
    with open(CONFIG_PATH, 'w') as f:
        yaml.dump(config, f, default_flow_style=False)
    
    print(f"✅ Config saved to {CONFIG_PATH}\n")

def check_ergo_node():
    """Check Ergo node status and provide fixes"""
    print("=== Checking Ergo Node ===\n")
    
    node_url = f"http://{ERGONODE_IP}:{ERGONODE_PORT}"
    
    # Test connectivity
    print(f"Testing {node_url}/info...")
    try:
        resp = requests.get(f"{node_url}/info", timeout=10)
        print(f"  Status: {resp.status_code}")
        
        if resp.status_code == 200:
            data = resp.json()
            print(f"  ✅ Node is responding!")
            print(f"  Network: {data.get('network')}")
            print(f"  Height: {data.get('fullHeight')}")
            print(f"  Explorer: {data.get('isExplorer')}")
            return True
        else:
            print(f"  ❌ Node returned {resp.status_code}")
            print(f"  Response: {resp.text[:100]}")
            return False
    except requests.exceptions.ConnectionError:
        print(f"  ❌ Connection refused - Node may be down")
        print(f"\n  To start the Ergo node:")
        print(f"    cd /home/n1ur0/Xergon-Network")
        print(f"    docker compose up -d ergo-node")
        return False
    except requests.exceptions.Timeout:
        print(f"  ❌ Connection timed out")
        print(f"  Check if the node is running and accessible")
        return False
    except Exception as e:
        print(f"  ❌ Error: {str(e)[:100]}")
        return False

def create_utxo_config():
    """Create UTXO configuration for Xergon"""
    print("\n=== Creating UTXO Configuration ===\n")
    
    xergon_dir = "/home/n1ur0/Xergon-Network"
    config_file = os.path.join(xergon_dir, "ergo-node-config.yaml")
    
    config = {
        "ergo_node": {
            "url": f"http://{ERGONODE_IP}:{ERGONODE_PORT}",
            "network": "testnet",
            "api_version": "v6.0.3",
            "endpoints": {
                "info": "/info",
                "blocks": "/blocks/lastHeaders/{n}",
                "transactions": "/transactions/unconfirmed",
                "boxes": "/boxes/unspent",  # May need different path
                "by_address": "/boxes/byAddress/{address}"
            },
            "fallback_endpoints": [
                "https://api.ergoplatform.com"
            ]
        },
        "mcp": {
            "ergo_knowledge": {
                "url": "https://ergo-knowledge-base.vercel.app/api/mcp",
                "headers": {
                    "Accept": "application/json, text/event-stream"
                },
                "methods": ["search", "get_concept", "list_projects"]
            },
            "ergo_transcript": {
                "url": "https://ergo-transcripts.vercel.app/api/mcp",
                "headers": {
                    "Accept": "application/json, text/event-stream"
                }
            }
        },
        "notes": {
            "utxo_issue": "UTXO endpoints may require Ergo Node Explorer plugin",
            "mcp_issue": "MCP server may need reconfiguration or different endpoint",
            "solution": [
                "1. Start Ergo node: docker compose up -d ergo-node",
                "2. Wait for sync: docker logs -f ergo-node",
                "3. Test endpoints: curl http://192.168.1.75:9052/info",
                "4. If UTXO fails, check node logs for Explorer plugin status",
                "5. For MCP, try alternative: https://api.ergoplatform.com"
            ]
        }
    }
    
    with open(config_file, 'w') as f:
        yaml.dump(config, f, default_flow_style=False)
    
    print(f"✅ Created {config_file}")
    print(f"\nKey configuration:")
    print(f"  Node URL: {config['ergo_node']['url']}")
    print(f"  Network: {config['ergo_node']['network']}")
    print(f"\nNext steps:")
    for step in config['notes']['solution']:
        print(f"  {step}")

def test_public_api():
    """Test public Ergo API as fallback"""
    print("\n=== Testing Public Ergo API ===\n")
    
    public_url = "https://api.ergoplatform.com"
    
    endpoints = [
        "/info",
        "/transactions/unconfirmed",
        "/blocks/lastHeaders/3"
    ]
    
    for endpoint in endpoints:
        try:
            resp = requests.get(f"{public_url}{endpoint}", timeout=10)
            print(f"{endpoint}: {resp.status_code}")
            if resp.status_code == 200:
                data = resp.json()
                if isinstance(data, list):
                    print(f"  → {len(data)} items")
                elif isinstance(data, dict):
                    if 'items' in data:
                        print(f"  → {len(data['items'])} items")
                    else:
                        print(f"  → OK")
        except Exception as e:
            print(f"{endpoint}: Error - {str(e)[:50]}")

def main():
    print("=" * 60)
    print("MCP & UTXO Node Configuration Fix")
    print("=" * 60)
    print()
    
    # Fix MCP config
    fix_mcp_config()
    
    # Check Ergo node
    node_ok = check_ergo_node()
    
    # Create UTXO config
    create_utxo_config()
    
    # Test public API
    test_public_api()
    
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    
    if not node_ok:
        print("\n⚠️  Ergo node is not responding!")
        print("\nRecommended actions:")
        print("  1. Check if node is running: docker ps | grep ergo")
        print("  2. Start node if stopped: docker compose up -d ergo-node")
        print("  3. Wait for sync: docker logs -f ergo-node")
        print("  4. Test: curl http://192.168.1.75:9052/info")
    else:
        print("\n✅ Ergo node is responding!")
    
    print("\n✅ MCP configuration updated")
    print("✅ UTXO config created at /home/n1ur0/Xergon-Network/ergo-node-config.yaml")
    print("\nUse public API (https://api.ergoplatform.com) as fallback if local node is down.")

if __name__ == "__main__":
    main()
