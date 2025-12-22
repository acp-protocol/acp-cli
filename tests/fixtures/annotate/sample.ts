/**
 * User authentication service.
 * Handles login, logout, and session management.
 *
 * @module AuthService
 * @category Security
 */

/**
 * Represents a user in the system.
 * @see UserRepository
 */
export interface User {
  id: string;
  email: string;
  /** User's hashed password - never expose this */
  passwordHash: string;
}

/**
 * Authentication configuration options.
 */
export interface AuthConfig {
  /** JWT token expiration in seconds */
  tokenExpiry: number;
  /** Enable refresh tokens */
  useRefreshTokens: boolean;
}

/**
 * Service for handling user authentication.
 *
 * @example
 * const auth = new AuthService(config);
 * const token = await auth.login(email, password);
 */
export class AuthService {
  private config: AuthConfig;

  /**
   * Creates a new AuthService instance.
   * @param config - Authentication configuration
   */
  constructor(config: AuthConfig) {
    this.config = config;
  }

  /**
   * Authenticates a user with email and password.
   *
   * @param email - User's email address
   * @param password - User's password
   * @returns JWT token if authentication succeeds
   * @throws {AuthError} If credentials are invalid
   * @deprecated Use authenticateWithMFA instead
   */
  async login(email: string, password: string): Promise<string> {
    // Implementation
    return "token";
  }

  /**
   * Validates a JWT token.
   * @param token - The token to validate
   * @returns True if token is valid
   */
  validateToken(token: string): boolean {
    return true;
  }

  /**
   * Logs out the current user.
   */
  logout(): void {
    // Clear session
  }
}

// Helper function without JSDoc - should get heuristic annotations
export function hashPassword(password: string): string {
  return password; // Simplified
}

// Private function - should not be annotated
function internalHelper(): void {
  // Internal use only
}
