import React from 'react';

interface AppLogoProps {
  size?: number;
  className?: string;
}

export const AppLogo: React.FC<AppLogoProps> = ({ size = 30, className }) => {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 512 512"
      width={size}
      height={size}
      className={className}
      style={{ display: 'block' }}
    >
      <defs>
        <linearGradient id="appLogoBgGradient" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" style={{ stopColor: '#f0f4ff', stopOpacity: 1 }} />
          <stop offset="100%" style={{ stopColor: '#e8edf5', stopOpacity: 1 }} />
        </linearGradient>
        <linearGradient id="appLogoPrinterBody" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" style={{ stopColor: '#4a90e2', stopOpacity: 1 }} />
          <stop offset="100%" style={{ stopColor: '#357abd', stopOpacity: 1 }} />
        </linearGradient>
        <filter id="appLogoSoftShadow" x="-20%" y="-20%" width="140%" height="140%">
          <feDropShadow dx="0" dy="8" stdDeviation="12" floodColor="#000000" floodOpacity="0.08" />
        </filter>
        <filter id="appLogoInnerGlow" x="-10%" y="-10%" width="120%" height="120%">
          <feDropShadow dx="0" dy="2" stdDeviation="4" floodColor="#ffffff" floodOpacity="0.3" />
        </filter>
      </defs>
      
      {/* Background with subtle gradient */}
      <rect width="512" height="512" rx="100" fill="url(#appLogoBgGradient)" />
      
      {/* Subtle decorative circles for visual interest */}
      <circle cx="420" cy="90" r="60" fill="#4a90e2" opacity="0.06" />
      <circle cx="90" cy="420" r="45" fill="#4a90e2" opacity="0.04" />
      <circle cx="400" cy="400" r="35" fill="#5ba8f5" opacity="0.05" />
      
      {/* Printer body with modern gradient and shadow */}
      <g filter="url(#appLogoSoftShadow)">
        <rect x="100" y="180" width="312" height="200" rx="24" fill="url(#appLogoPrinterBody)" />
      </g>
      
      {/* Paper input tray - elevated with shadow */}
      <g filter="url(#appLogoSoftShadow)">
        <rect x="150" y="90" width="212" height="140" rx="16" fill="#ffffff" />
        <rect x="150" y="90" width="212" height="140" rx="16" fill="url(#appLogoInnerGlow)" />
      </g>
      
      {/* Paper output tray */}
      <g filter="url(#appLogoSoftShadow)">
        <rect x="130" y="310" width="252" height="130" rx="16" fill="#ffffff" />
      </g>
      
      {/* Paper details - subtle gray lines */}
      <rect x="175" y="335" width="180" height="12" rx="6" fill="#c8d6e5" />
      <rect x="175" y="360" width="140" height="12" rx="6" fill="#c8d6e5" />
      <rect x="175" y="385" width="160" height="12" rx="6" fill="#c8d6e5" />
      
      {/* Paper input lines - lighter */}
      <rect x="175" y="135" width="150" height="8" rx="4" fill="#e8edf5" />
      <rect x="175" y="155" width="120" height="8" rx="4" fill="#e8edf5" />
      <rect x="175" y="175" width="130" height="8" rx="4" fill="#e8edf5" />
      
      {/* Status indicator with modern design */}
      <circle cx="380" cy="240" r="14" fill="#4cd964" />
      <circle cx="380" cy="240" r="10" fill="#5ae070" />
      
      {/* Power button accent */}
      <rect x="370" y="265" width="20" height="8" rx="4" fill="#ffffff" opacity="0.8" />
    </svg>
  );
};