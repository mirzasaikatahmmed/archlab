import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import type { ServerProfile } from "../bindings";
import { api } from "../ipc";
import { stripNulls } from "../lib/numbers";
import {
  serverProfileSchema,
  toServerProfile,
  type ServerProfileValues,
} from "../schemas/serverProfile";
import { FormField } from "./FormField";
import { Icon } from "./Icon";
import { Section } from "./Section";

interface Props {
  value: ServerProfile;
  onChange: (profile: ServerProfile) => void;
}

export function ServerProfileForm({ value, onChange }: Props) {
  const {
    register,
    reset,
    subscribe,
    formState: { errors },
  } = useForm<ServerProfileValues>({
    resolver: zodResolver(serverProfileSchema),
    defaultValues: stripNulls(value),
    mode: "onChange",
  });
  const [notes, setNotes] = useState<string[]>([]);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [detecting, setDetecting] = useState(false);

  useEffect(() => {
    return subscribe({
      formState: { values: true },
      callback: ({ values }) => {
        const parsed = serverProfileSchema.safeParse(values);
        if (parsed.success) {
          onChange(toServerProfile(parsed.data));
        }
      },
    });
  }, [subscribe, onChange]);

  const detect = async () => {
    setDetecting(true);
    setDetectError(null);
    try {
      const detected = await api.detectMachine();
      const profile = stripNulls(detected.profile);
      reset(profile);
      onChange(toServerProfile(profile));
      setNotes(detected.notes);
    } catch (err) {
      setDetectError(err instanceof Error ? err.message : String(err));
    } finally {
      setDetecting(false);
    }
  };

  const num = (name: Parameters<typeof register>[0]) => register(name, { valueAsNumber: true });

  return (
    <form
      className="form-page"
      onSubmit={(e) => {
        e.preventDefault();
      }}
      noValidate
    >
      <div className="page-actions">
        <button
          type="button"
          onClick={() => {
            void detect();
          }}
          disabled={detecting}
        >
          <Icon name="detect" />
          {detecting ? "Detecting…" : "Detect this machine"}
        </button>
        <span className="muted">Fills the form from this computer. Review the notes it lists.</span>
      </div>
      {detectError && <p role="alert">Detection failed: {detectError}</p>}
      {notes.length > 0 && (
        <ul className="notes" aria-label="Detection notes">
          {notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}

      <Section title="Compute" description="Sets how fast each request is processed.">
        <FormField label="Operating system" htmlFor="os" error={errors.os?.message}>
          <select id="os" {...register("os")}>
            <option value="linux">Linux</option>
            <option value="macOs">macOS</option>
            <option value="windows">Windows</option>
            <option value="other">Other</option>
          </select>
        </FormField>
        <FormField
          label="Virtualization"
          htmlFor="virtualization"
          error={errors.virtualization?.message}
        >
          <select id="virtualization" {...register("virtualization")}>
            <option value="bareMetal">Bare metal</option>
            <option value="vm">Virtual machine</option>
            <option value="container">Container</option>
          </select>
        </FormField>
        <FormField label="CPU cores" htmlFor="cpuCores" error={errors.cpuCores?.message}>
          <input id="cpuCores" type="number" step="1" {...num("cpuCores")} />
        </FormField>
        <FormField label="CPU clock (GHz)" htmlFor="cpuGhz" error={errors.cpuGhz?.message}>
          <input id="cpuGhz" type="number" step="0.1" {...num("cpuGhz")} />
        </FormField>
        <FormField label="RAM (GB)" htmlFor="ramGb" error={errors.ramGb?.message}>
          <input id="ramGb" type="number" step="0.25" {...num("ramGb")} />
        </FormField>
      </Section>

      <Section title="Storage" description="Limits databases and anything that waits on disk.">
        <FormField label="Disk type" htmlFor="diskKind" error={errors.disk?.kind?.message}>
          <select id="diskKind" {...register("disk.kind")}>
            <option value="hdd">HDD</option>
            <option value="ssd">SSD</option>
            <option value="nvme">NVMe</option>
          </select>
        </FormField>
        <FormField label="Disk IOPS" htmlFor="diskIops" error={errors.disk?.iops?.message}>
          <input id="diskIops" type="number" step="1" {...num("disk.iops")} />
        </FormField>
      </Section>

      <Section title="Network" description="Adds delay to every hop between nodes.">
        <FormField
          label="Network bandwidth (Mbps)"
          htmlFor="bandwidthMbps"
          error={errors.network?.bandwidthMbps?.message}
        >
          <input id="bandwidthMbps" type="number" step="any" {...num("network.bandwidthMbps")} />
        </FormField>
        <FormField
          label="Network latency (ms)"
          htmlFor="latencyMs"
          error={errors.network?.latencyMs?.message}
        >
          <input id="latencyMs" type="number" step="any" {...num("network.latencyMs")} />
        </FormField>
      </Section>

      <Section title="Limits" description="Caps how many connections a node can hold open.">
        <FormField label="File descriptor limit" htmlFor="fdLimit" error={errors.fdLimit?.message}>
          <input id="fdLimit" type="number" step="1" {...num("fdLimit")} />
        </FormField>
      </Section>
    </form>
  );
}
