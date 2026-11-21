import { useEffect } from "react";
import { useForm, useWatch } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import type { TrafficProfile } from "../bindings";
import { stripNulls } from "../lib/numbers";
import {
  defaultSpike,
  toTrafficProfile,
  trafficProfileSchema,
  type SpikeKind,
  type TrafficProfileValues,
} from "../schemas/trafficProfile";
import { FormField } from "./FormField";
import { Section } from "./Section";

interface Props {
  value: TrafficProfile;
  onChange: (profile: TrafficProfile) => void;
}

export function TrafficProfileEditor({ value, onChange }: Props) {
  const {
    register,
    control,
    subscribe,
    setValue,
    formState: { errors },
  } = useForm<TrafficProfileValues>({
    resolver: zodResolver(trafficProfileSchema),
    defaultValues: stripNulls(value),
    mode: "onChange",
  });

  useEffect(() => {
    return subscribe({
      formState: { values: true },
      callback: ({ values }) => {
        const parsed = trafficProfileSchema.safeParse(values);
        if (parsed.success) {
          onChange(toTrafficProfile(parsed.data));
        }
      },
    });
  }, [subscribe, onChange]);

  const readRatio = useWatch({ control, name: "readRatio" });
  const spikeKind = useWatch({ control, name: "spike.kind" });
  const writePct = Number.isFinite(readRatio) ? Math.round((1 - readRatio) * 100) : null;
  const spikeErrors = errors.spike as Partial<Record<string, { message?: string }>> | undefined;
  const num = (name: Parameters<typeof register>[0]) =>
    register(name, { valueAsNumber: true, shouldUnregister: true });

  return (
    <form
      className="form-page"
      onSubmit={(e) => {
        e.preventDefault();
      }}
      noValidate
    >
      <Section title="Load" description="How much traffic arrives, and what it asks for.">
        <FormField
          label="Concurrent users"
          htmlFor="concurrentUsers"
          error={errors.concurrentUsers?.message}
        >
          <input id="concurrentUsers" type="number" step="1" {...num("concurrentUsers")} />
        </FormField>
        <FormField
          label="Requests per second"
          htmlFor="requestsPerSecond"
          error={errors.requestsPerSecond?.message}
        >
          <input id="requestsPerSecond" type="number" step="any" {...num("requestsPerSecond")} />
        </FormField>
        <FormField
          label="Read ratio (0–1)"
          htmlFor="readRatio"
          hint={writePct === null ? undefined : `${String(writePct)}% writes`}
          error={errors.readRatio?.message}
        >
          <input id="readRatio" type="number" step="0.01" {...num("readRatio")} />
        </FormField>
      </Section>

      <Section title="Spikes" description="Sudden bursts on top of the steady load.">
        <FormField label="Spike pattern" htmlFor="spikeKind">
          <select
            id="spikeKind"
            value={spikeKind}
            onChange={(e) => {
              setValue("spike", defaultSpike(e.target.value as SpikeKind), {
                shouldValidate: true,
              });
            }}
          >
            <option value="none">None</option>
            <option value="step">Single step</option>
            <option value="periodic">Periodic</option>
          </select>
        </FormField>
        {spikeKind === "step" && (
          <FormField label="Spike start (s)" htmlFor="spikeAtS" error={spikeErrors?.atS?.message}>
            <input id="spikeAtS" type="number" step="any" {...num("spike.atS")} />
          </FormField>
        )}
        {spikeKind === "periodic" && (
          <FormField
            label="Spike period (s)"
            htmlFor="spikePeriodS"
            error={spikeErrors?.periodS?.message}
          >
            <input id="spikePeriodS" type="number" step="any" {...num("spike.periodS")} />
          </FormField>
        )}
        {spikeKind !== "none" && (
          <>
            <FormField
              label="Spike multiplier"
              htmlFor="spikeMultiplier"
              error={spikeErrors?.multiplier?.message}
            >
              <input id="spikeMultiplier" type="number" step="any" {...num("spike.multiplier")} />
            </FormField>
            <FormField
              label="Spike duration (s)"
              htmlFor="spikeDurationS"
              error={spikeErrors?.durationS?.message}
            >
              <input id="spikeDurationS" type="number" step="any" {...num("spike.durationS")} />
            </FormField>
          </>
        )}
      </Section>
    </form>
  );
}
