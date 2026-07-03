/**
 * Full-screen loading state shown while the first index scan runs (~5-7s).
 * Keeps the window from feeling empty/dead on cold launch.
 */
import { Loader2, Sparkles } from "lucide-react";
import { motion } from "framer-motion";

export function FirstRun({ message }: { message: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center bg-canvas">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ type: "spring", stiffness: 200, damping: 20 }}
        className="flex flex-col items-center"
      >
        <div className="relative mb-5">
          <div className="flex h-14 w-14 items-center justify-center rounded-xl bg-accent shadow-lg">
            <Sparkles size={26} className="text-white" />
          </div>
        </div>
        <h1 className="text-[17px] font-semibold text-ink">Claude Sessions</h1>
        <p className="mt-1 text-[13px] text-ink-3">{message}</p>
        <Loader2 size={18} className="mt-5 animate-spin text-accent" />
      </motion.div>
    </div>
  );
}
