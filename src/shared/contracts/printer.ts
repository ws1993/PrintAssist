export type CapabilitySupport = 'supported' | 'unsupported' | 'unknown';

export type PrinterOperationalState = 'ready' | 'offline' | 'error' | 'unknown';

export interface PrinterCapability {
  support: CapabilitySupport;
  source: 'driver' | 'system' | 'unavailable';
  detail?: string;
}

export interface SystemPrinter {
  name: string;
  portName: string | null;
  isDefault: boolean;
  state: PrinterOperationalState;
  statusCode: number;
  color: PrinterCapability;
  duplex: PrinterCapability;
  error?: string;
}
