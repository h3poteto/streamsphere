import { Inter } from "next/font/google";
import { useState } from "react";
import { useRouter } from "next/router";

const inter = Inter({ subsets: ["latin"] });

export default function Home() {
  const [room, setRoom] = useState<string>("");

  const router = useRouter();

  const openRoom = () => {
    router.push(`/room?room=${room}`);
  };

  return (
    <main
      className={`flex min-h-screen flex-col items-center justify-between p-24 ${inter.className}`}
    >
      <div className="flex flex-row items-center">
        <input
          placeholder="The room name"
          value={room}
          onChange={(v) => setRoom(v.target.value)}
        />
        <button onClick={openRoom}>Join</button>
      </div>
    </main>
  );
}
