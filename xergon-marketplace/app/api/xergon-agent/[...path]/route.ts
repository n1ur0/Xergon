import { NextRequest, NextResponse } from "next/server";

const AGENT_BASE =
  process.env.XERGON_AGENT_BASE ?? "http://127.0.0.1:9099";

type Params = { path: string[] };

async function proxyRequest(
  req: NextRequest,
  method: string,
  pathSegments: string[]
): Promise<NextResponse> {
  const upstreamPath = "/" + pathSegments.join("/");
  const url = `${AGENT_BASE}${upstreamPath}`;

  // Forward search params if present
  const searchParams = req.nextUrl.searchParams.toString();
  const fullUrl = searchParams ? `${url}?${searchParams}` : url;

  const headers: Record<string, string> = {};
  const contentType = req.headers.get("content-type");
  if (contentType) headers["content-type"] = contentType;

  const res = await fetch(fullUrl, {
    method,
    headers,
    body: method !== "GET" && method !== "HEAD" ? req.body : undefined,
    // @ts-expect-error Node 18+ supports duplex
    duplex: method !== "GET" && method !== "HEAD" ? "half" : undefined,
  });

  const body = await res.arrayBuffer();

  return new NextResponse(body, {
    status: res.status,
    statusText: res.statusText,
    headers: {
      "content-type": res.headers.get("content-type") ?? "application/json",
    },
  });
}

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<Params> }
) {
  const { path } = await params;
  return proxyRequest(req, "GET", path);
}

export async function POST(
  req: NextRequest,
  { params }: { params: Promise<Params> }
) {
  const { path } = await params;
  return proxyRequest(req, "POST", path);
}

export async function PUT(
  req: NextRequest,
  { params }: { params: Promise<Params> }
) {
  const { path } = await params;
  return proxyRequest(req, "PUT", path);
}

export async function DELETE(
  req: NextRequest,
  { params }: { params: Promise<Params> }
) {
  const { path } = await params;
  return proxyRequest(req, "DELETE", path);
}
