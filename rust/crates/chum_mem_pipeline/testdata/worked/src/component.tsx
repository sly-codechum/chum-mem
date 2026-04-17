import React, { useState, useEffect } from "react";
import { fetchUser, User } from "../api/users";

// WHY: Keep props minimal — derive computed values inside the component
interface ProfileCardProps {
  userId: string;
  showAvatar?: boolean;
}

function formatDisplayName(user: User): string {
  return user.nickname ?? `${user.firstName} ${user.lastName}`;
}

/** Primary profile card shown on dashboards and search results. */
const ProfileCard: React.FC<ProfileCardProps> = ({ userId, showAvatar = true }) => {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // NOTE: AbortController handles unmount-during-fetch race condition
    const ctrl = new AbortController();
    fetchUser(userId, { signal: ctrl.signal })
      .then(setUser)
      .finally(() => setLoading(false));
    return () => ctrl.abort();
  }, [userId]);

  if (loading) return <div className="skeleton" />;
  if (!user) return <p>User not found</p>;

  return (
    <div className="profile-card">
      {showAvatar && <img src={user.avatarUrl} alt={formatDisplayName(user)} />}
      <h2>{formatDisplayName(user)}</h2>
      <span className="email">{user.email}</span>
    </div>
  );
};

export default ProfileCard;
