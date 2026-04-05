"use client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface NetworkUptimeProps {
  uptime: number; // percentage 0-100
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getColor(uptime: number): string {
  if (uptime >= 99) return "#10b981"; // accent-500 (green)
  if (uptime >= 95) return "#f59e0b"; // amber-500 (yellow)
  return "#ef4444"; // danger-500 (red)
}

function getLabel(uptime: number): string {
  if (uptime >= 99) return "Healthy";
  if (uptime >= 95) return "Degraded";
  return "Critical";
}

function getLabelColor(uptime: number): string {
  if (uptime >= 99) return "text-accent-500";
  if (uptime >= 95) return "text-amber-500";
  return "text-danger-500";
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function NetworkUptime({ uptime }: NetworkUptimeProps) {
  const radius = 44;
  const strokeWidth = 6;
  const normalizedRadius = radius - strokeWidth / 2;
  const circumference = normalizedRadius * 2 * Math.PI;
  const offset = circumference - (uptime / 100) * circumference;
  const color = getColor(uptime);
  const size = (radius + strokeWidth) * 2;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 flex flex-col items-center">
      <h2 className="text-base font-semibold text-surface-900 mb-3">
        Network Uptime
      </h2>

      <div className="relative" style={{ width: size, height: size }}>
        <svg
          width={size}
          height={size}
          viewBox={`0 0 ${size} ${size}`}
          role="img"
          aria-label={`Network uptime: ${uptime}%`}
        >
          {/* Background circle */}
          <circle
            stroke="currentColor"
            strokeOpacity={0.1}
            fill="transparent"
            strokeWidth={strokeWidth}
            r={normalizedRadius}
            cx={radius + strokeWidth / 2}
            cy={radius + strokeWidth / 2}
            className="text-surface-800"
          />

          {/* Progress arc */}
          <circle
            stroke={color}
            fill="transparent"
            strokeWidth={strokeWidth}
            strokeLinecap="round"
            strokeDasharray={`${circumference} ${circumference}`}
            strokeDashoffset={offset}
            r={normalizedRadius}
            cx={radius + strokeWidth / 2}
            cy={radius + strokeWidth / 2}
            transform={`rotate(-90 ${radius + strokeWidth / 2} ${radius + strokeWidth / 2})`}
            style={{ transition: "stroke-dashoffset 1s ease-in-out" }}
          />
        </svg>

        {/* Center label */}
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="text-xl font-bold text-surface-900">
            {uptime.toFixed(1)}%
          </span>
          <span className={`text-[10px] font-medium ${getLabelColor(uptime)}`}>
            {getLabel(uptime)}
          </span>
        </div>
      </div>

      <p className="text-xs text-surface-800/40 mt-2">Last 30 days</p>
    </div>
  );
}
