import { NextRequest, NextResponse } from "next/server";

/**
 * Next.js Edge Middleware -- operator route protection.
 *
 * Reads the `xergon-auth-token` cookie. If present and looks valid
 * (non-empty, base64-decodes to an object with a publicKey field),
 * the request passes through. Otherwise, /operator/* routes are
 * redirected to the home page with ?auth=required so the UI can
 * prompt wallet connection.
 *
 * Public routes (everything except /operator/*) are always allowed.
 */

const AUTH_COOKIE = "xergon-auth-token";

export const config = {
  // Protect all /operator routes (including sub-paths)
  matcher: ["/operator/:path*"],
};

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  // Only protect /operator/* paths (matcher already scopes this, but be safe)
  if (!pathname.startsWith("/operator")) {
    return NextResponse.next();
  }

  // Check for auth token in cookie
  const token = request.cookies.get(AUTH_COOKIE)?.value;

  if (!token) {
    const url = request.nextUrl.clone();
    url.pathname = "/";
    url.searchParams.set("auth", "required");
    return NextResponse.redirect(url);
  }

  // Validate the token is a non-trivial string
  try {
    const decoded = atob(token);
    const parsed = JSON.parse(decoded);
    if (!parsed.pk || typeof parsed.pk !== "string") {
      throw new Error("invalid");
    }
  } catch {
    const url = request.nextUrl.clone();
    url.pathname = "/";
    url.searchParams.set("auth", "required");
    return NextResponse.redirect(url);
  }

  return NextResponse.next();
}
